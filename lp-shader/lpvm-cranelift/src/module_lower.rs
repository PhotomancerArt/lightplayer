//! Shared LPIR → Cranelift lowering for any [`cranelift_module::Module`] (JIT or object).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use cranelift_codegen::ir::{FuncRef, Signature, StackSlot, StackSlotData, StackSlotKind, types};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module};
use lpir::FloatMode;
use lpir::lpir_module::LpirModule;
use lpir::types::FuncId as LpirFuncId;

use crate::builtins::{self, LpirBuiltinFuncIds};
use crate::compile_options::{CompileOptions, MemoryStrategy};
use crate::emit::{self, LpirBuiltinRefs, translate_function};
use crate::error::{CompileError, CompilerError};

/// Order used when declaring and defining user functions (affects `FuncId` / object symbol order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LpirFuncEmitOrder {
    /// Same order as [`LpirModule::functions`] (host JIT).
    Source,
    /// Lexicographic by LPIR function name (used for `ObjectModule` symbol order).
    #[cfg_attr(
        not(feature = "riscv32-object"),
        allow(
            dead_code,
            reason = "only constructed when feature riscv32-object is on"
        )
    )]
    Name,
}

/// Result of lowering LPIR into a module before target-specific finalization.
pub(crate) struct LoweredLpirModule {
    pub func_ids: Vec<FuncId>,
    pub func_names: Vec<String>,
    pub signatures: BTreeMap<String, Signature>,
    /// LPIR scalar return words per function (for StructReturn ABIs where `Signature::returns` is empty).
    pub logical_return_words: BTreeMap<String, usize>,
    pub ir_param_counts: Vec<u16>,
    pub name_to_index: BTreeMap<String, usize>,
    pub call_conv: CallConv,
    pub pointer_type: types::Type,
    pub float_mode: FloatMode,
}

/// Declare imports, declare user functions, and define bodies. Caller runs `finalize_definitions` or `finish`.
pub(crate) fn lower_lpir_into_module<M: Module>(
    module: &mut M,
    ir: &LpirModule,
    options: CompileOptions,
    order: LpirFuncEmitOrder,
) -> Result<LoweredLpirModule, CompilerError> {
    let mode = options.float_mode;
    if mode == FloatMode::F32 && !ir.imports.is_empty() {
        return Err(CompilerError::Codegen(CompileError::unsupported(
            "LPIR imports require FloatMode::Q32 in lpvm-cranelift",
        )));
    }

    let call_conv = module.isa().default_call_conv();
    let pointer_type = module.isa().pointer_type();

    let import_func_ids = if mode == FloatMode::Q32 {
        builtins::declare_module_imports(module, ir, pointer_type)
            .map_err(CompilerError::Codegen)?
    } else {
        Vec::new()
    };

    let lpir_builtin_ids: Option<LpirBuiltinFuncIds> = if mode == FloatMode::Q32 {
        Some(
            builtins::declare_lpir_opcode_builtins(module, pointer_type)
                .map_err(CompilerError::Codegen)?,
        )
    } else {
        None
    };

    let func_id_to_ir_rank: BTreeMap<LpirFuncId, usize> = ir
        .functions
        .keys()
        .enumerate()
        .map(|(i, k)| (*k, i))
        .collect();

    let indices: Vec<LpirFuncId> = match (order, options.memory_strategy) {
        (_, MemoryStrategy::LowMemory) => {
            let mut v: Vec<LpirFuncId> = ir.functions.keys().copied().collect();
            v.sort_by(|a, b| {
                ir.functions[b]
                    .body
                    .len()
                    .cmp(&ir.functions[a].body.len())
            });
            v
        }
        (LpirFuncEmitOrder::Source, _) => ir.functions.keys().copied().collect(),
        (LpirFuncEmitOrder::Name, _) => {
            let mut v: Vec<LpirFuncId> = ir.functions.keys().copied().collect();
            v.sort_by(|a, b| ir.functions[a].name.cmp(&ir.functions[b].name));
            v
        }
    };

    let mut func_ids = Vec::with_capacity(indices.len());
    let mut signatures = BTreeMap::new();
    let mut logical_return_words = BTreeMap::new();
    let mut func_names = Vec::with_capacity(indices.len());
    let mut ir_param_counts = Vec::with_capacity(indices.len());

    let callee_struct_return: alloc::vec::Vec<bool> = ir
        .functions
        .values()
        .map(|f| emit::signature_uses_struct_return(module.isa(), f))
        .collect();

    for &fid in &indices {
        let f = &ir.functions[&fid];
        logical_return_words.insert(f.name.clone(), f.return_types.len());
        let sig = emit::signature_for_ir_func(f, call_conv, mode, pointer_type, module.isa());
        let id = module
            .declare_function(&f.name, Linkage::Export, &sig)
            .map_err(|e| {
                CompilerError::Codegen(CompileError::cranelift(alloc::format!(
                    "declare {}: {e}",
                    f.name
                )))
            })?;
        signatures.insert(f.name.clone(), sig);
        func_names.push(f.name.clone());
        ir_param_counts.push(f.param_count);
        func_ids.push(id);
    }

    let mut name_to_index = BTreeMap::new();
    for (j, name) in func_names.iter().enumerate() {
        name_to_index.insert(name.clone(), j);
    }

    // Map LPIR function rank (BTree key order) -> Cranelift FuncId for local calls.
    let mut id_at_ir: Vec<Option<FuncId>> = (0..ir.functions.len()).map(|_| None).collect();
    for (emit_pos, &fid) in indices.iter().enumerate() {
        let r = func_id_to_ir_rank[&fid];
        id_at_ir[r] = Some(func_ids[emit_pos]);
    }

    let mut ctx = module.make_context();

    for (emit_pos, &fid) in indices.iter().enumerate() {
        let f = &ir.functions[&fid];
        let fid = func_ids[emit_pos];
        ctx.clear();
        let uses_struct_return = emit::signature_uses_struct_return(module.isa(), f);
        ctx.func.signature =
            emit::signature_for_ir_func(f, call_conv, mode, pointer_type, module.isa());
        let mut func_ctx = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
            let entry = builder.create_block();
            builder.append_block_params_for_function_params(entry);
            builder.switch_to_block(entry);

            let slots: Vec<StackSlot> = f
                .slots
                .iter()
                .map(|sd| {
                    builder.func.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        sd.size,
                        0,
                    ))
                })
                .collect();

            let func_refs: Vec<FuncRef> = id_at_ir
                .iter()
                .map(|id| {
                    module.declare_func_in_func(id.expect("func id for IR index"), builder.func)
                })
                .collect();

            let import_func_refs: Vec<FuncRef> = import_func_ids
                .iter()
                .map(|id| module.declare_func_in_func(*id, builder.func))
                .collect();

            let lpir_builtins = lpir_builtin_ids.as_ref().map(|ids| LpirBuiltinRefs {
                fadd: module.declare_func_in_func(ids.fadd, builder.func),
                fsub: module.declare_func_in_func(ids.fsub, builder.func),
                fmul: module.declare_func_in_func(ids.fmul, builder.func),
                fdiv: module.declare_func_in_func(ids.fdiv, builder.func),
                fsqrt: module.declare_func_in_func(ids.fsqrt, builder.func),
                fnearest: module.declare_func_in_func(ids.fnearest, builder.func),
            });

            let vreg_wide_addr = emit::vreg_wide_addr_chain(f);
            let emit_ctx = emit::EmitCtx {
                func_refs: &func_refs,
                import_func_refs: &import_func_refs,
                slots: &slots,
                ir,
                func_id_to_ir_rank: &func_id_to_ir_rank,
                pointer_type,
                vreg_wide_addr,
                float_mode: mode,
                lpir_builtins,
                uses_struct_return,
                callee_struct_return: &callee_struct_return,
            };

            translate_function(f, &mut builder, &emit_ctx).map_err(|e| {
                CompilerError::Codegen(match e {
                    CompileError::Unsupported(s) => {
                        CompileError::unsupported(alloc::format!("in function `{}`: {s}", f.name))
                    }
                    CompileError::Cranelift(s) => {
                        CompileError::cranelift(alloc::format!("in function `{}`: {s}", f.name))
                    }
                })
            })?;
            builder.finalize();
        }
        module.define_function(fid, &mut ctx).map_err(|e| {
            CompilerError::Codegen(CompileError::cranelift(alloc::format!(
                "define {}: {e}",
                f.name
            )))
        })?;

        if options.memory_strategy == MemoryStrategy::LowMemory {
            module.clear_context(&mut ctx);
        } else {
            ctx.clear();
        }
    }

    Ok(LoweredLpirModule {
        func_ids,
        func_names,
        signatures,
        logical_return_words,
        ir_param_counts,
        name_to_index,
        call_conv,
        pointer_type,
        float_mode: mode,
    })
}
