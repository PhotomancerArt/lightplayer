//! WASM compilation options.

use lps_naga::FloatMode;

/// Options for GLSL-to-WASM compilation.
#[derive(Debug, Clone)]
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
