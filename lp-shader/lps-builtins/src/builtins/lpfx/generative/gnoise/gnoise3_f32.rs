//! 3D Gradient Noise function (float implementation - stub).

use crate::builtins::lpfx::generative::gnoise::gnoise3_q32::__lp_lpfx_gnoise3_q32;
use lps_q32::q32::Q32;

#[lpfx_impl_macro::lpfx_impl(f32, "float lpfx_gnoise(vec3 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfx_gnoise3_f32(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let x_q32 = Q32::from_f32_wrapping(x);
    let y_q32 = Q32::from_f32_wrapping(y);
    let z_q32 = Q32::from_f32_wrapping(z);
    let result_fixed =
        __lp_lpfx_gnoise3_q32(x_q32.to_fixed(), y_q32.to_fixed(), z_q32.to_fixed(), seed);
    Q32::from_fixed(result_fixed).to_f32()
}
