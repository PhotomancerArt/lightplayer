//! 1D Simplex noise function (float implementation - stub).
//!
//! This is a stub implementation that will be replaced with a proper float implementation later.
//! For now, it calls the q32 version with conversion.

use crate::builtins::lpfx::generative::snoise::snoise1_q32::__lpfx_snoise1_q32;
use crate::glsl::q32::types::q32::Q32;

/// 1D Simplex noise function (float version).
///
/// # Arguments
/// * `x` - X coordinate as f32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value approximately in range [-1, 1] as f32
#[lpfx_impl_macro::lpfx_impl(f32, "float lpfx_snoise(float x, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_snoise1_f32(x: f32, seed: u32) -> f32 {
    // Stub: convert to q32, call q32 version, convert back
    let x_q32 = Q32::from_f32(x);
    let result_fixed = __lpfx_snoise1_q32(x_q32.to_fixed(), seed);
    Q32::from_fixed(result_fixed).to_f32()
}
