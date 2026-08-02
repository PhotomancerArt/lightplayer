//! `LpvmEngine` trait — compilation and shared memory.

use lpir::lpir_module::LpirModule;
use lpir::{CompilerConfig, FloatMode};
use lps_shared::LpsModuleSig;

use crate::compile_job::BoxedLpvmCompileJob;
use crate::memory::LpvmMemory;
use crate::module::LpvmModule;

/// Everything a caller chooses **per compile**, as opposed to per engine.
///
/// [`CompilerConfig`] is the middle-end (inlining, texture bounds);
/// [`FloatMode`] is the numeric mode the backend emits for, and it is here
/// rather than on the engine because the authored `float_mode` slot lives on
/// each shader node — one engine compiles Fixed and Float modules side by side
/// (`docs/adr/2026-08-01-float-mode-as-a-compiler-parameter.md`, decision 1).
///
/// Not every engine can honour every mode. Ask
/// [`LpvmEngine::supports_float_mode`] **before** compiling: it is the
/// capability query, and the never-silent-fallback rule
/// (`docs/adr/2026-07-09-preview-fidelity-tiers.md` §4) makes an unsupported
/// request an error rather than a quiet downgrade to Fixed.
/// There is deliberately no `Default`, for the same reason
/// [`crate::LpvmEngine::supports_float_mode`] exists: which numeric mode a
/// shader compiles in is a decision some caller made, and a defaulted field
/// would let it be made by omission. Use [`LpvmCompileParams::from_config`]
/// when the answer really is "the shipped mode".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LpvmCompileParams {
    /// Middle-end LPIR pass settings.
    pub config: CompilerConfig,
    /// Numeric mode this module is emitted in.
    pub float_mode: FloatMode,
}

impl LpvmCompileParams {
    /// Params carrying `config` in the engine's default numeric mode
    /// ([`FloatMode::Q32`], the shipped mode on every board).
    #[must_use]
    pub fn from_config(config: CompilerConfig) -> Self {
        Self {
            config,
            float_mode: FloatMode::Q32,
        }
    }
}

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

    /// Whether this engine can compile a module in `mode`.
    ///
    /// The capability query callers ask **before** handing a
    /// [`LpvmCompileParams`] to [`Self::compile_with_params`]. It exists so an
    /// unsupported request fails with a message naming the backend, rather
    /// than either compiling the wrong numeric mode or dying deep in lowering.
    ///
    /// The default answers [`FloatMode::Q32`] only — the shipped mode on every
    /// board, and the only one the default [`Self::compile_with_params`] can
    /// honour, since that one ignores `params.float_mode` entirely. **An
    /// engine that widens this must also override
    /// [`Self::compile_with_params`]**, or it will claim a mode it then
    /// silently does not emit. Engines whose mode is fixed at construction
    /// answer "the mode I was built with" instead.
    fn supports_float_mode(&self, mode: FloatMode) -> bool {
        matches!(mode, FloatMode::Q32)
    }

    /// Compile with explicit per-call [`LpvmCompileParams`] (middle-end passes
    /// and numeric mode).
    ///
    /// Default implementation ignores `params` and delegates to
    /// [`Self::compile`]. That is sound only alongside the default
    /// [`Self::supports_float_mode`], which admits Q32 alone — see its docs.
    /// Backends that honor per-call settings (native RV32/Xtensa JIT, wasm)
    /// override.
    fn compile_with_params(
        &self,
        ir: &LpirModule,
        meta: &LpsModuleSig,
        params: &LpvmCompileParams,
    ) -> Result<Self::Module, Self::Error> {
        debug_assert!(
            self.supports_float_mode(params.float_mode),
            "compile_with_params was asked for a float mode this engine does not support; \
             callers must consult supports_float_mode first"
        );
        self.compile(ir, meta)
    }

    /// Start a resumable compile job when the backend supports incremental compilation.
    ///
    /// Default implementation returns `None`, allowing callers to fall back to synchronous
    /// [`Self::compile_with_params`].
    fn start_compile_job<'a>(
        &'a self,
        _ir: LpirModule,
        _meta: LpsModuleSig,
        _params: LpvmCompileParams,
    ) -> Option<BoxedLpvmCompileJob<'a, Self::Module, Self::Error>> {
        None
    }

    /// Shared memory allocator for this engine (textures, cross-shader data).
    fn memory(&self) -> &dyn LpvmMemory;
}
