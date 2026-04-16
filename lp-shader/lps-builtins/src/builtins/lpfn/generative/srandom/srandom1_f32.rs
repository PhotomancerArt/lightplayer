//! 1D Signed Random function (float implementation - stub).

use crate::builtins::lpfn::generative::srandom::srandom1_q32::__lp_lpfn_srandom1_q32;
use lps_q32::q32::Q32;

#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_srandom(float x, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_srandom1_f32(x: f32, seed: u32) -> f32 {
    let x_q32 = Q32::from_f32_wrapping(x);
    let result_fixed = __lp_lpfn_srandom1_q32(x_q32.to_fixed(), seed);
    Q32::from_fixed(result_fixed).to_f32()
}
