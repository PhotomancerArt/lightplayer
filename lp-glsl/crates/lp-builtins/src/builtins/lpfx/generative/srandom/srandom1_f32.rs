//! 1D Signed Random function (float implementation - stub).

use crate::builtins::lpfx::generative::srandom::srandom1_q32::__lpfx_srandom1_q32;
use crate::glsl::q32::types::q32::Q32;

#[lpfx_impl_macro::lpfx_impl(f32, "float lpfx_srandom(float x, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_srandom1_f32(x: f32, seed: u32) -> f32 {
    let x_q32 = Q32::from_f32(x);
    let result_fixed = __lpfx_srandom1_q32(x_q32.to_fixed(), seed);
    Q32::from_fixed(result_fixed).to_f32()
}
