//! 2D Worley noise function value variant (float implementation - stub).
//!
//! This is a stub implementation that will be replaced with a proper float implementation later.
//! For now, it calls the q32 version with conversion.

use crate::builtins::lpfx::generative::worley::worley2_value_q32::__lpfx_worley2_value_q32;
use crate::glsl::q32::types::q32::Q32;

/// 2D Worley noise function value variant (float version).
///
/// # Arguments
/// * `x` - X coordinate as f32
/// * `y` - Y coordinate as f32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Hash value of nearest cell approximately in range [-1, 1] as f32
#[lpfx_impl_macro::lpfx_impl(f32, "float lpfx_worley_value(vec2 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_worley2_value_f32(x: f32, y: f32, seed: u32) -> f32 {
    // Stub: convert to q32, call q32 version, convert back
    let x_q32 = Q32::from_f32(x);
    let y_q32 = Q32::from_f32(y);
    let result_fixed = __lpfx_worley2_value_q32(x_q32.to_fixed(), y_q32.to_fixed(), seed);
    Q32::from_fixed(result_fixed).to_f32()
}
