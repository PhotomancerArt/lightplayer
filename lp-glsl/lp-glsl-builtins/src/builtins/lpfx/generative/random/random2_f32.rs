//! 2D Random function (float implementation - stub).
//!
//! This is a stub implementation that calls the q32 version with conversion.

use crate::builtins::lpfx::generative::random::random2_q32::__lpfx_random2_q32;
use crate::glsl::q32::types::q32::Q32;

/// 2D Random function (float version).
///
/// # Arguments
/// * `x` - X coordinate as f32
/// * `y` - Y coordinate as f32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Random value in [0, 1] range as f32
#[lpfx_impl_macro::lpfx_impl(f32, "float lpfx_random(vec2 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_random2_f32(x: f32, y: f32, seed: u32) -> f32 {
    // Convert to q32, call q32 version, convert back
    let x_q32 = Q32::from_f32(x);
    let y_q32 = Q32::from_f32(y);
    let result_fixed = __lpfx_random2_q32(x_q32.to_fixed(), y_q32.to_fixed(), seed);
    Q32::from_fixed(result_fixed).to_f32()
}
