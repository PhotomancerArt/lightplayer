//! GLSL → Naga → LPIR → JIT pipeline.

use lpir::{GlslModuleMeta, IrModule};

use crate::error::CompilerError;
use crate::jit_module::{CompileOptions, JitModule, build_jit_module};

/// Compile GLSL source to a host JIT module (Q32 recommended; F32 for import-free IR).
pub fn jit(source: &str, options: &CompileOptions) -> Result<JitModule, CompilerError> {
    let naga =
        lp_glsl_naga::compile(source).map_err(|e| CompilerError::Parse(alloc::format!("{e}")))?;
    let (ir, meta) = lp_glsl_naga::lower(&naga).map_err(CompilerError::Lower)?;
    drop(naga);
    build_jit_module(&ir, meta, *options)
}

/// Build JIT from borrowed LPIR. [`GlslModuleMeta::default`] is used; [`JitModule::call`] needs
/// metadata from [`jit`] or [`jit_from_ir_owned`].
pub fn jit_from_ir(ir: &IrModule, options: &CompileOptions) -> Result<JitModule, CompilerError> {
    build_jit_module(ir, GlslModuleMeta::default(), *options)
}

/// Owned LPIR + metadata (e.g. from [`lp_glsl_naga::lower`]) for a full [`JitModule::call`] surface.
pub fn jit_from_ir_owned(
    ir: IrModule,
    meta: GlslModuleMeta,
    options: &CompileOptions,
) -> Result<JitModule, CompilerError> {
    build_jit_module(&ir, meta, *options)
}
