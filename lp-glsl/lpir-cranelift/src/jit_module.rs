//! Host JIT module construction from LPIR.

use alloc::vec::Vec;

use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use lpir::module::IrModule;

use crate::emit::{self, translate_function};
use crate::error::CompileError;

/// Build a host JIT module from LPIR (imports must be empty; functions must be linear-only).
///
/// Returns the module and [`FuncId`] values in the same order as [`IrModule::functions`].
pub fn jit_from_ir(ir: &IrModule) -> Result<(JITModule, Vec<FuncId>), CompileError> {
    if !ir.imports.is_empty() {
        return Err(CompileError::unsupported(
            "modules with imports are not supported yet",
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

    let call_conv = isa.default_call_conv();
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
            builder.seal_block(entry);
            translate_function(f, &mut builder)?;
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
