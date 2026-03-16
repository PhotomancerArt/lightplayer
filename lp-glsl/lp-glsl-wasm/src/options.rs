//! WASM compilation options.

use lp_glsl_frontend::DecimalFormat;

/// Options for GLSL-to-WASM compilation.
#[derive(Debug, Clone)]
pub struct WasmOptions {
    /// Numeric format: Q32 (fixed-point i32) or Float (f32).
    pub decimal_format: DecimalFormat,
    /// Maximum number of errors to collect before stopping.
    pub max_errors: usize,
}

impl Default for WasmOptions {
    fn default() -> Self {
        Self {
            decimal_format: DecimalFormat::Q32,
            max_errors: lp_glsl_frontend::DEFAULT_MAX_ERRORS,
        }
    }
}
