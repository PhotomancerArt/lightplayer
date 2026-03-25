//! Host JIT module construction from LPIR.

use alloc::vec::Vec;

use cranelift_codegen::ir::{FuncRef, StackSlot, StackSlotData, StackSlotKind};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use lpir::module::IrModule;
use lpir::op::Op;

use crate::emit::{self, translate_function};
use crate::error::CompileError;

/// Build a host JIT module from LPIR.
///
/// Returns the module and [`FuncId`] values in the same order as [`IrModule::functions`].
/// Import calls fail at emit time with [`CompileError::Unsupported`]; other functions in the
/// same module may still compile if they do not call imports.
pub fn jit_from_ir(ir: &IrModule) -> Result<(JITModule, Vec<FuncId>), CompileError> {
    let mut flag_builder = settings::builder();
    flag_builder
        .set("regalloc_algorithm", "single_pass")
        .map_err(|e| CompileError::cranelift(alloc::format!("regalloc_algorithm: {e}")))?;
    let flags = settings::Flags::new(flag_builder);

    let isa = cranelift_native::builder()
        .map_err(|m| CompileError::cranelift(alloc::format!("native ISA detection: {m}")))?
        .finish(flags)
        .map_err(|e| CompileError::cranelift(alloc::format!("ISA: {e}")))?;

    let call_conv = isa.default_call_conv();
    let pointer_type = isa.pointer_type();
    let mut jit_module = JITModule::new(JITBuilder::with_isa(
        isa,
        cranelift_module::default_libcall_names(),
    ));

    let mut fn_ids = Vec::with_capacity(ir.functions.len());
    for f in &ir.functions {
        let sig = emit::signature_for_ir_func(f, call_conv);
        let id = jit_module
            .declare_function(&f.name, Linkage::Export, &sig)
            .map_err(|e| CompileError::cranelift(alloc::format!("declare {}: {e}", f.name)))?;
        fn_ids.push(id);
    }

    let mut ctx = jit_module.make_context();

    for (f, fid) in ir.functions.iter().zip(&fn_ids) {
        ctx.clear();
        ctx.func.signature = emit::signature_for_ir_func(f, call_conv);
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
                slots: &slots,
                ir,
                pointer_type,
                vreg_is_stack_addr,
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
