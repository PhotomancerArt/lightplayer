//! Numeric mode: Q32 (i32 fixed-point) vs Float (f32).

use lp_glsl_frontend::DecimalFormat;

/// Numeric mode for WASM codegen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmNumericMode {
    /// Q16.16 fixed-point, i32 WASM ops
    Q32,
    /// Native f32 WASM ops
    Float,
}

impl From<DecimalFormat> for WasmNumericMode {
    fn from(f: DecimalFormat) -> Self {
        match f {
            DecimalFormat::Q32 => WasmNumericMode::Q32,
            DecimalFormat::Float => WasmNumericMode::Float,
        }
    }
}
