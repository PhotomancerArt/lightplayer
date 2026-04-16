//! GLSL → Naga → LPIR → JIT pipeline (GLSL front-end: enable crate feature `glsl`).

use lpir::LpirModule;
use lps_shared::LpsModuleSig;

use crate::compile_options::CompileOptions;
use crate::error::CompilerError;
use crate::lpvm_module::CraneliftModule;

/// Compile GLSL source to a JIT module (Q32 recommended; F32 for import-free IR).
///
/// Requires the `glsl` feature (`lps-frontend`). Works on `#!no_std` + `alloc` targets (e.g. ESP32);
/// host builds add `std` for native ISA autodetection via `cranelift-native`.
///
/// This is a convenience helper that combines the frontend (`lps_frontend`) with the backend
/// (`CraneliftEngine`). For direct LPIR compilation, use [`jit_from_ir`] or [`jit_from_ir_owned`].
#[cfg(feature = "glsl")]
pub fn jit(source: &str, options: &CompileOptions) -> Result<CraneliftModule, CompilerError> {
    let naga =
        lps_frontend::compile(source).map_err(|e| CompilerError::Parse(alloc::format!("{e}")))?;
    let (ir, meta) = lps_frontend::lower(&naga).map_err(CompilerError::Lower)?;
    drop(naga);
    CraneliftModule::compile(&ir, &meta, *options)
}

/// Build JIT from borrowed LPIR. [`LpsModuleSig::default`] is used; [`CraneliftModule::call`] needs
/// metadata from [`jit`] or [`jit_from_ir_owned`].
pub fn jit_from_ir(
    ir: &LpirModule,
    options: &CompileOptions,
) -> Result<CraneliftModule, CompilerError> {
    CraneliftModule::compile(ir, &LpsModuleSig::default(), *options)
}

/// Owned LPIR + metadata (e.g. from [`lps_frontend::lower`]) for a full [`CraneliftModule::call`] surface.
pub fn jit_from_ir_owned(
    ir: LpirModule,
    meta: LpsModuleSig,
    options: &CompileOptions,
) -> Result<CraneliftModule, CompilerError> {
    CraneliftModule::compile(&ir, &meta, *options)
}

#[cfg(feature = "riscv32-object")]
pub use crate::object_module::object_bytes_from_ir;
