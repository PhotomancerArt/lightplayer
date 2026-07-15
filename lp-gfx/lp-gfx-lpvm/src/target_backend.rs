//! Per-target LPVM engine selection and construction.
//!
//! Exactly one CPU engine is compiled per target (no Cargo feature): this is
//! the *guaranteed* backend of the lp-gfx doctrine. Optional accelerated
//! backends live in their own crates (`lp-gfx-wgpu`) and are selected at
//! runtime creation, never silently.

use lp_shader::ShaderFrontend;

use crate::lpvm_graphics::LpvmGraphics;

/// The LPVM engine compiled for this target.
#[cfg(target_arch = "riscv32")]
pub type TargetLpvmEngine = lpvm_native::NativeJitEngine;

/// The LPVM engine compiled for this target.
#[cfg(target_arch = "wasm32")]
pub type TargetLpvmEngine = lpvm_wasm::rt_browser::BrowserLpvmEngine;

/// The LPVM engine compiled for this target.
#[cfg(not(any(target_arch = "riscv32", target_arch = "wasm32")))]
pub type TargetLpvmEngine = lpvm_wasm::rt_wasmtime::WasmLpvmEngine;

/// CPU graphics backend for this target.
pub type TargetLpvmGraphics = LpvmGraphics<TargetLpvmEngine>;

/// RV32 native JIT (`lpvm-native` `rt_jit`): in-process machine-code JIT, no
/// Cranelift, no ELF link. The only backend on firmware targets
/// (`fw-esp32`, `fw-emu`).
#[cfg(target_arch = "riscv32")]
impl TargetLpvmGraphics {
    /// `frontend` is the host's GLSL-frontend product decision (see
    /// [`lp_gfx::LpGraphics::glsl_frontend`]).
    #[must_use]
    pub fn new(frontend: ShaderFrontend) -> Self {
        lps_builtins::ensure_builtins_referenced();
        let mut table = lpvm_native::BuiltinTable::new();
        table.populate();
        let backend = lpvm_native::NativeJitEngine::new(
            alloc::sync::Arc::new(table),
            lpvm_native::NativeCompileOptions::default(),
        );
        Self::from_engine(backend, "lpvm-native::rt_jit", frontend)
    }
}

/// Wasm32 guest (`lpvm-wasm` `rt_browser`): runs emitted shader WASM via the
/// host JS `WebAssembly.Module` / `Instance` API.
#[cfg(target_arch = "wasm32")]
impl TargetLpvmGraphics {
    /// `frontend` is the host's GLSL-frontend product decision (see
    /// [`lp_gfx::LpGraphics::glsl_frontend`]).
    #[must_use]
    pub fn new(frontend: ShaderFrontend) -> Self {
        let backend =
            lpvm_wasm::rt_browser::BrowserLpvmEngine::new(lpvm_wasm::WasmOptions::default())
                .expect("BrowserLpvmEngine::new with default WasmOptions");
        Self::from_engine(backend, "lpvm-wasm::rt_browser", frontend)
    }
}

/// Host (`lpvm-wasm` `rt_wasmtime`): all of LPIR → WASM → wasmtime JIT
/// happens in-process. Pre-grows linear memory once per engine (see
/// [`lpvm_wasm::WasmOptions::host_memory_pages`]) so cached buffer host
/// pointers stay valid.
#[cfg(not(any(target_arch = "riscv32", target_arch = "wasm32")))]
impl TargetLpvmGraphics {
    /// `frontend` is the host's GLSL-frontend product decision (see
    /// [`lp_gfx::LpGraphics::glsl_frontend`]).
    #[must_use]
    pub fn new(frontend: ShaderFrontend) -> Self {
        let backend =
            lpvm_wasm::rt_wasmtime::WasmLpvmEngine::new(lpvm_wasm::WasmOptions::default())
                .expect("WasmLpvmEngine::new with default WasmOptions");
        Self::from_engine(backend, "lpvm-wasm::rt_wasmtime", frontend)
    }
}
