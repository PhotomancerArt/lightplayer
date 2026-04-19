//! `LpvmEngine` trait — compilation and shared memory.

use lpir::CompilerConfig;
use lpir::lpir_module::LpirModule;
use lps_shared::LpsModuleSig;

use crate::memory::LpvmMemory;
use crate::module::LpvmModule;

/// Backend engine: compiles LPIR and owns shared memory for cross-module data.
///
/// Implementations typically hold configuration (e.g. wasmtime `Engine`) and a
/// [`LpvmMemory`] implementation. All modules produced by [`Self::compile`]
/// share the same memory arena (textures, globals). Host code allocates with
/// [`Self::memory`]; guests see [`crate::LpvmBuffer::guest_base`] (or [`crate::LpvmPtr`]) via uniforms.
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
    fn compile(&self, ir: &LpirModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error>;

    /// Compile with an explicit per-call [`CompilerConfig`] (middle-end passes, Q32 mode, etc.).
    ///
    /// Default implementation ignores `config` and delegates to [`Self::compile`]. Backends that
    /// honor per-call settings (e.g. Cranelift JIT, native RV32 JIT) must override; others may keep
    /// the default until they gain config threading.
    fn compile_with_config(
        &self,
        ir: &LpirModule,
        meta: &LpsModuleSig,
        _config: &CompilerConfig,
    ) -> Result<Self::Module, Self::Error> {
        self.compile(ir, meta)
    }

    /// Shared memory allocator for this engine (textures, cross-shader data).
    fn memory(&self) -> &dyn LpvmMemory;
}
