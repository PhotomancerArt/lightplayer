//! Shared LPIR → Cranelift lowering for any [`cranelift_module::Module`].

use alloc::vec::Vec;
use lp_collection::VecMap;

use cranelift_codegen::ir::{FuncRef, StackSlot, StackSlotData, StackSlotKind};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module};
use lpir::FloatMode;
use lpir::lpir_module::LpirModule;
use lpir::types::FuncId as LpirFuncId;

use crate::builtins::{self, LpirBuiltinFuncIds};
use crate::compile_options::{CompileOptions, MemoryStrategy};
use crate::emit::{self, LpirBuiltinRefs, translate_function};
use crate::error::{CompileError, CompilerError};

/// Declare imports, declare user functions, and define bodies. Caller runs `finalize_definitions` or `finish`.
pub(crate) fn lower_lpir_into_module<M: Module>(
    module: &mut M,
    ir: &LpirModule,
    options: CompileOptions,
) -> Result<(), CompilerError> {
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

    let func_id_to_ir_rank: VecMap<LpirFuncId, usize> = ir
        .functions
        .keys()
        .enumerate()
        .map(|(i, k)| (*k, i))
        .collect();

    // Lexicographic by LPIR function name for a stable object symbol order; LowMemory
    // defines the largest bodies first so peak context memory is bounded by the biggest one.
    let indices: Vec<LpirFuncId> = match options.memory_strategy {
        MemoryStrategy::LowMemory => {
            let mut v: Vec<LpirFuncId> = ir.functions.keys().copied().collect();
            v.sort_by(|a, b| ir.functions[b].body.len().cmp(&ir.functions[a].body.len()));
            v
        }
        _ => {
            let mut v: Vec<LpirFuncId> = ir.functions.keys().copied().collect();
            v.sort_by(|a, b| ir.functions[a].name.cmp(&ir.functions[b].name));
            v
        }
    };

    let mut func_ids = Vec::with_capacity(indices.len());

    for &fid in &indices {
        let f = &ir.functions[&fid];
        let sig = emit::signature_for_ir_func(f, call_conv, mode, pointer_type, module.isa());
        let id = module
            .declare_function(&f.name, Linkage::Export, &sig)
            .map_err(|e| {
                CompilerError::Codegen(CompileError::cranelift(alloc::format!(
                    "declare {}: {e}",
                    f.name
                )))
            })?;
        func_ids.push(id);
    }

    // Map LPIR function rank (BTree key order) -> Cranelift FuncId for local calls.
    let mut id_at_ir: Vec<Option<FuncId>> = (0..ir.functions.len()).map(|_| None).collect();
    for (emit_pos, &fid) in indices.iter().enumerate() {
        let r = func_id_to_ir_rank[&fid];
        id_at_ir[r] = Some(func_ids[emit_pos]);
    }

    // Per-IR-rank flag: does the callee's Cranelift signature use StructReturn?
    // Indexed by IR rank (VecMap key order), matching `func_id_to_ir_rank` and `id_at_ir`.
    let mut callee_struct_return: Vec<bool> = vec![false; ir.functions.len()];
    for (fid, f) in ir.functions.iter() {
        let r = func_id_to_ir_rank[fid];
        callee_struct_return[r] = emit::signature_uses_struct_return(module.isa(), f);
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

    Ok(())
}
