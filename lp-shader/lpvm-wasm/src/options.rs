//! WASM compilation options.

use lpir::{CompilerConfig, FloatMode};

/// Options for LPIR-to-WASM compilation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmOptions {
    /// Numeric format: Q32 (fixed-point i32) or Float (f32).
    pub float_mode: FloatMode,

    /// Middle-end LPIR pass settings (inline, etc.).
    pub config: CompilerConfig,

    /// Wasmtime host runtime: number of 64 KiB wasm pages to pre-grow the
    /// linear memory to at engine construction.
    ///
    /// The wasmtime backend (`rt_wasmtime`) caches raw host pointers in
    /// `LpvmBuffer::native`. If `Memory::grow` is called after the first
    /// allocation, wasmtime is free to relocate the linear memory and any
    /// previously-cached `native` pointer becomes a use-after-free hazard.
    /// To avoid this, the wasmtime engine pre-grows the linear memory once
    /// at construction to `host_memory_pages` and never grows it again;
    /// `WasmtimeLpvmMemory::alloc` returns `OutOfMemory` past the cap.
    ///
    /// Default = 1024 pages = 64 MiB. Ignored on the wasm32 (`rt_browser`)
    /// runtime, which uses `WebAssembly.Memory`'s grow on demand.
    pub host_memory_pages: u32,

    /// When true (default), insert fuel checks: check-then-decrement at every
    /// loop back-edge and check-only at every function entry (mirrors the rv32
    /// design in `docs/adr/2026-07-20-lpvm-native-fuel.md`). Always on for the
    /// wasm hosts; `false` exists for tests and perf comparison only.
    pub fuel: bool,
}

impl Default for WasmOptions {
    fn default() -> Self {
        Self {
            float_mode: FloatMode::Q32,
            config: CompilerConfig::default(),
            host_memory_pages: 1024,
            fuel: true,
        }
    }
}
