//! `LpvmEngine` trait — compilation and shared memory.

use lpir::module::IrModule;
use lps_shared::LpsModuleSig;

use crate::memory::LpvmMemory;
use crate::module::LpvmModule;

/// Backend engine: compiles LPIR and owns shared memory for cross-module data.
///
/// Implementations typically hold configuration (e.g. wasmtime `Engine`) and a
/// [`LpvmMemory`] implementation. All modules produced by [`Self::compile`]
/// share the same memory arena (textures, globals). Host code allocates with
/// [`Self::memory`]; guests see [`crate::ShaderPtr::guest_value`] via uniforms.
///
/// # Per-instance vs shared
///
/// [`crate::VmContext`] is per shader instance (fuel, trap handler). Shared
/// heap data is **not** stored in `VmContext`; use this memory API instead.
pub trait LpvmEngine {
    /// Compiled module type produced by this engine.
    type Module: LpvmModule;

    /// Error type for compilation failures.
    type Error: core::fmt::Display;

    /// Compile an LPIR module into a runnable module.
    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error>;

    /// Shared memory allocator for this engine (textures, cross-shader data).
    fn memory(&self) -> &dyn LpvmMemory;
}
