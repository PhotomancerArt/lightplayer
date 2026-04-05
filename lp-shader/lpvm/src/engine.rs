//! `LpvmEngine` trait - backend configuration and compilation.

use lpir::module::IrModule;
use lps_shared::LpsModuleSig;

use crate::module::LpvmModule;

/// Backend engine that compiles LPIR modules and creates execution contexts.
///
/// Implementations hold shared configuration and cached resources (e.g.,
/// wasmtime Engine with parsed builtins). A single engine can compile
/// multiple modules.
pub trait LpvmEngine {
    /// Compiled module type produced by this engine.
    type Module: LpvmModule;

    /// Error type for compilation failures.
    type Error: core::fmt::Display;

    /// Compile an LPIR module into a runnable module.
    ///
    /// The `meta` parameter provides the function signatures and other metadata
    /// needed for the compiled artifact. Backends should store this to support
    /// the `signatures()` method on `LpvmModule`.
    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error>;
}
