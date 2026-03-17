//! WASM compilation options.

use lp_glsl_frontend::FloatMode;

/// Options for GLSL-to-WASM compilation.
#[derive(Debug, Clone)]
pub struct WasmOptions {
    /// Numeric format: Q32 (fixed-point i32) or Float (f32).
    pub float_mode: FloatMode,
    /// Maximum number of errors to collect before stopping.
    pub max_errors: usize,
}

impl Default for WasmOptions {
    fn default() -> Self {
        Self {
            float_mode: FloatMode::Q32,
            max_errors: lp_glsl_frontend::DEFAULT_MAX_ERRORS,
        }
    }
}
