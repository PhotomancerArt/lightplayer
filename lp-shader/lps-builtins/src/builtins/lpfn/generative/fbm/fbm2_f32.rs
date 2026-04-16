//! 2D Fractal Brownian Motion noise function (float implementation - stub).

use crate::builtins::lpfn::generative::fbm::fbm2_q32::__lp_lpfn_fbm2_q32;
use lps_q32::q32::Q32;

#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_fbm(vec2 p, int octaves, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_fbm2_f32(x: f32, y: f32, octaves: i32, seed: u32) -> f32 {
    let x_q32 = Q32::from_f32_wrapping(x);
    let y_q32 = Q32::from_f32_wrapping(y);
    let result_fixed = __lp_lpfn_fbm2_q32(x_q32.to_fixed(), y_q32.to_fixed(), octaves, seed);
    Q32::from_fixed(result_fixed).to_f32()
}
