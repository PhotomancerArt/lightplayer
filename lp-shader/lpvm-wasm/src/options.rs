//! WASM compilation options.

use lpir::FloatMode;

/// Options for LPIR-to-WASM compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WasmOptions {
    /// Numeric format: Q32 (fixed-point i32) or Float (f32).
    pub float_mode: FloatMode,
}

impl Default for WasmOptions {
    fn default() -> Self {
        Self {
            float_mode: FloatMode::Q32,
        }
    }
}
