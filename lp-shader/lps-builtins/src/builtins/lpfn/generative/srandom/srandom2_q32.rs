//! 2D Signed Random function.
//!
//! Returns values in [-1, 1] range using -1.0 + 2.0 * random(p, seed)
//!
//! This function was derived from LYGIA's srandom.glsl, which uses the Prosperity License.
//! However, this is a trivial mathematical transform (srandom = -1 + 2*random) applied to
//! our MIT-licensed lpfn_random2 function. The formula itself is not copyrightable - it is
//! basic arithmetic. The underlying random function is MIT-licensed (David Hoskins).

use crate::builtins::lpfn::generative::random::random2_q32::lpfn_random2;
use lps_q32::q32::Q32;
use lps_q32::vec2_q32::Vec2Q32;

/// 2D Signed Random function
///
/// # Arguments
/// * `p` - Input coordinates as Vec2Q32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Random value in [-1, 1] range as Q32
#[inline(always)]
pub fn lpfn_srandom2(p: Vec2Q32, seed: u32) -> Q32 {
    let random_val = lpfn_random2(p, seed);
    // -1.0 + 2.0 * random_val
    Q32::from_f32_wrapping(-1.0) + Q32::from_f32_wrapping(2.0) * random_val
}

/// 2D Signed Random function (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate as i32 (Q32 fixed-point)
/// * `y` - Y coordinate as i32 (Q32 fixed-point)
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Random value in [-1, 1] range as i32 (Q32 fixed-point format)
#[lpfn_impl_macro::lpfn_impl(q32, "float lpfn_srandom(vec2 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_srandom2_q32(x: i32, y: i32, seed: u32) -> i32 {
    lpfn_srandom2(Vec2Q32::new(Q32::from_fixed(x), Q32::from_fixed(y)), seed).to_fixed()
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    #[test]
    fn test_srandom2_range() {
        let result = __lp_lpfn_srandom2_q32(
            Q32::from_f32_wrapping(42.0).to_fixed(),
            Q32::from_f32_wrapping(10.0).to_fixed(),
            123,
        );
        let val = Q32::from_fixed(result).to_f32();
        assert!(
            val >= -1.0 && val <= 1.0,
            "Srandom should be in [-1, 1] range"
        );
    }
}
