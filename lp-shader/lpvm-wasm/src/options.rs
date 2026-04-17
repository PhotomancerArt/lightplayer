//! WASM compilation options.

use lpir::{CompilerConfig, FloatMode};

/// Options for LPIR-to-WASM compilation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmOptions {
    /// Numeric format: Q32 (fixed-point i32) or Float (f32).
    pub float_mode: FloatMode,

    /// Middle-end LPIR pass settings (inline, etc.).
    pub config: CompilerConfig,
}

impl Default for WasmOptions {
    fn default() -> Self {
        Self {
            float_mode: FloatMode::Q32,
            config: CompilerConfig::default(),
        }
    }
}
