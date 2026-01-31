//! 2D Signed Random function (float implementation - stub).

use crate::builtins::lpfx::generative::srandom::srandom2_q32::__lpfx_srandom2_q32;
use crate::glsl::q32::types::q32::Q32;

#[lpfx_impl_macro::lpfx_impl(f32, "float lpfx_srandom(vec2 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_srandom2_f32(x: f32, y: f32, seed: u32) -> f32 {
    let x_q32 = Q32::from_f32(x);
    let y_q32 = Q32::from_f32(y);
    let result_fixed = __lpfx_srandom2_q32(x_q32.to_fixed(), y_q32.to_fixed(), seed);
    Q32::from_fixed(result_fixed).to_f32()
}
