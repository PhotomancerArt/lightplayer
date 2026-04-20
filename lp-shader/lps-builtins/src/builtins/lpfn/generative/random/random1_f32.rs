//! 1D Random function (float implementation - stub).
//!
//! This is a stub implementation that calls the q32 version with conversion.

use crate::builtins::lpfn::generative::random::random1_q32::__lp_lpfn_random1_q32;
use lps_q32::q32::Q32;

/// 1D Random function (float version).
///
/// # Arguments
/// * `x` - X coordinate as f32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Random value in [0, 1] range as f32
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_random(float x, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_random1_f32(x: f32, seed: u32) -> f32 {
    // Convert to q32, call q32 version, convert back
    let x_q32 = Q32::from_f32_wrapping(x);
    let result_fixed = __lp_lpfn_random1_q32(x_q32.to_fixed(), seed);
    Q32::from_fixed(result_fixed).to_f32()
}
