//! GLSL → Naga → LPIR → JIT pipeline (GLSL front-end: enable crate feature `glsl`).

use lpir::IrModule;
use lps_shared::LpsModuleSig;

use crate::compile_options::CompileOptions;
use crate::error::CompilerError;
use crate::jit_module::{JitModule, build_jit_module};

/// Compile GLSL source to a JIT module (Q32 recommended; F32 for import-free IR).
///
/// Requires the `glsl` feature (`lps-frontend`). Works on `#!no_std` + `alloc` targets (e.g. ESP32);
/// host builds add `std` for native ISA autodetection via `cranelift-native`.
#[cfg(feature = "glsl")]
pub fn jit(source: &str, options: &CompileOptions) -> Result<JitModule, CompilerError> {
    let naga =
        lps_frontend::compile(source).map_err(|e| CompilerError::Parse(alloc::format!("{e}")))?;
    let (ir, meta) = lps_frontend::lower(&naga).map_err(CompilerError::Lower)?;
    drop(naga);
    build_jit_module(&ir, meta, *options)
}

/// Build JIT from borrowed LPIR. [`LpsModuleSig::default`] is used; [`JitModule::call`] needs
/// metadata from [`jit`] or [`jit_from_ir_owned`].
pub fn jit_from_ir(ir: &IrModule, options: &CompileOptions) -> Result<JitModule, CompilerError> {
    build_jit_module(ir, LpsModuleSig::default(), *options)
}

/// Owned LPIR + metadata (e.g. from [`lps_frontend::lower`]) for a full [`JitModule::call`] surface.
pub fn jit_from_ir_owned(
    ir: IrModule,
    meta: LpsModuleSig,
    options: &CompileOptions,
) -> Result<JitModule, CompilerError> {
    build_jit_module(&ir, meta, *options)
}

#[cfg(feature = "riscv32-object")]
pub use crate::object_module::object_bytes_from_ir;
