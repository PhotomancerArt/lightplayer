//! Host JIT module construction from LPIR.

use alloc::vec::Vec;

use cranelift_codegen::ir::{FuncRef, StackSlot, StackSlotData, StackSlotKind};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use lpir::FloatMode;
use lpir::module::IrModule;
use lpir::op::Op;

use crate::builtins;
use crate::emit::{self, LpirBuiltinRefs, translate_function};
use crate::error::CompileError;

/// Build a host JIT module from LPIR.
///
/// Returns the module and [`FuncId`] values in the same order as [`IrModule::functions`].
pub fn jit_from_ir(
    ir: &IrModule,
    mode: FloatMode,
) -> Result<(JITModule, Vec<FuncId>), CompileError> {
    if mode == FloatMode::F32 && !ir.imports.is_empty() {
        return Err(CompileError::unsupported(
            "LPIR imports require FloatMode::Q32 in lpir-cranelift",
        ));
    }

    let mut flag_builder = settings::builder();
    flag_builder
        .set("regalloc_algorithm", "single_pass")
        .map_err(|e| CompileError::cranelift(alloc::format!("regalloc_algorithm: {e}")))?;
    let flags = settings::Flags::new(flag_builder);

    let isa = cranelift_native::builder()
        .map_err(|m| CompileError::cranelift(alloc::format!("native ISA detection: {m}")))?
        .finish(flags)
        .map_err(|e| CompileError::cranelift(alloc::format!("ISA: {e}")))?;

    let mut jit_builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    jit_builder.symbol_lookup_fn(builtins::symbol_lookup_fn());
    let mut jit_module = JITModule::new(jit_builder);

    let call_conv = jit_module.isa().default_call_conv();
    let pointer_type = jit_module.isa().pointer_type();

    let import_func_ids = if mode == FloatMode::Q32 {
        builtins::declare_module_imports(&mut jit_module, ir, pointer_type)?
    } else {
        Vec::new()
    };

    let lpir_builtin_ids = if mode == FloatMode::Q32 {
        Some(builtins::declare_lpir_opcode_builtins(
            &mut jit_module,
            pointer_type,
        )?)
    } else {
        None
    };

    let mut fn_ids = Vec::with_capacity(ir.functions.len());
    for f in &ir.functions {
        let sig = emit::signature_for_ir_func(f, call_conv, mode);
        let id = jit_module
            .declare_function(&f.name, Linkage::Export, &sig)
            .map_err(|e| CompileError::cranelift(alloc::format!("declare {}: {e}", f.name)))?;
        fn_ids.push(id);
    }

    let mut ctx = jit_module.make_context();

    for (f, fid) in ir.functions.iter().zip(&fn_ids) {
        ctx.clear();
        ctx.func.signature = emit::signature_for_ir_func(f, call_conv, mode);
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

            let func_refs: Vec<FuncRef> = fn_ids
                .iter()
                .map(|id| jit_module.declare_func_in_func(*id, builder.func))
                .collect();

            let import_func_refs: Vec<FuncRef> = import_func_ids
                .iter()
                .map(|id| jit_module.declare_func_in_func(*id, builder.func))
                .collect();

            let lpir_builtins = lpir_builtin_ids.as_ref().map(|ids| LpirBuiltinRefs {
                fadd: jit_module.declare_func_in_func(ids.fadd, builder.func),
                fsub: jit_module.declare_func_in_func(ids.fsub, builder.func),
                fmul: jit_module.declare_func_in_func(ids.fmul, builder.func),
                fdiv: jit_module.declare_func_in_func(ids.fdiv, builder.func),
                fsqrt: jit_module.declare_func_in_func(ids.fsqrt, builder.func),
                fnearest: jit_module.declare_func_in_func(ids.fnearest, builder.func),
            });

            let mut vreg_is_stack_addr = vec![false; f.vreg_types.len()];
            for op in &f.body {
                if let Op::SlotAddr { dst, .. } = op {
                    let i = dst.0 as usize;
                    if let Some(slot) = vreg_is_stack_addr.get_mut(i) {
                        *slot = true;
                    }
                }
            }

            let emit_ctx = emit::EmitCtx {
                func_refs: &func_refs,
                import_func_refs: &import_func_refs,
                slots: &slots,
                ir,
                pointer_type,
                vreg_is_stack_addr,
                float_mode: mode,
                lpir_builtins,
            };

            translate_function(f, &mut builder, &emit_ctx)?;
            builder.finalize();
        }
        jit_module
            .define_function(*fid, &mut ctx)
            .map_err(|e| CompileError::cranelift(alloc::format!("define {}: {e}", f.name)))?;
    }

    jit_module
        .finalize_definitions()
        .map_err(|e| CompileError::cranelift(alloc::format!("finalize_definitions: {e}")))?;

    Ok((jit_module, fn_ids))
}
