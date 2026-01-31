//! 1D Signed Random function.
//!
//! Returns values in [-1, 1] range using -1.0 + 2.0 * random(x, seed)

use crate::builtins::lpfx::generative::random::random1_q32::lpfx_random1;
use crate::glsl::q32::types::q32::Q32;

/// 1D Signed Random function
///
/// # Arguments
/// * `x` - Input coordinate as Q32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Random value in [-1, 1] range as Q32
#[inline(always)]
pub fn lpfx_srandom1(x: Q32, seed: u32) -> Q32 {
    let random_val = lpfx_random1(x, seed);
    // -1.0 + 2.0 * random_val
    Q32::from_f32(-1.0) + Q32::from_f32(2.0) * random_val
}

/// 1D Signed Random function (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate as i32 (Q32 fixed-point)
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Random value in [-1, 1] range as i32 (Q32 fixed-point format)
#[lpfx_impl_macro::lpfx_impl(q32, "float lpfx_srandom(float x, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_srandom1_q32(x: i32, seed: u32) -> i32 {
    lpfx_srandom1(Q32::from_fixed(x), seed).to_fixed()
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    #[test]
    fn test_srandom1_range() {
        let result = __lpfx_srandom1_q32(Q32::from_f32(42.0).to_fixed(), 123);
        let val = Q32::from_fixed(result).to_f32();
        assert!(
            val >= -1.0 && val <= 1.0,
            "Srandom should be in [-1, 1] range"
        );
    }
}
