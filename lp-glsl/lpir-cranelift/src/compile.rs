//! GLSL → Naga → LPIR → JIT pipeline (GLSL front-end: enable crate feature `glsl`).

use lp_glsl_abi::GlslModuleMeta;
use lpir::IrModule;

use crate::compile_options::CompileOptions;
use crate::error::CompilerError;
use crate::jit_module::{JitModule, build_jit_module};

/// Compile GLSL source to a JIT module (Q32 recommended; F32 for import-free IR).
///
/// Requires the `glsl` feature (`lp-glsl-naga`). Works on `#!no_std` + `alloc` targets (e.g. ESP32);
/// host builds add `std` for native ISA autodetection via `cranelift-native`.
#[cfg(feature = "glsl")]
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

#[cfg(feature = "riscv32-emu")]
pub use crate::emu_run::run_lpir_function_i32;
#[cfg(feature = "riscv32-emu")]
pub use crate::object_module::object_bytes_from_ir;
