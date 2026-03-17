//! Numeric mode: Q32 (i32 fixed-point) vs Float (f32).

use lp_glsl_frontend::FloatMode;

/// Numeric mode for WASM codegen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmNumericMode {
    /// Q16.16 fixed-point, i32 WASM ops
    Q32,
    /// Native f32 WASM ops
    Float,
}

impl From<FloatMode> for WasmNumericMode {
    fn from(f: FloatMode) -> Self {
        match f {
            FloatMode::Q32 => WasmNumericMode::Q32,
            FloatMode::Float => WasmNumericMode::Float,
        }
    }
}
