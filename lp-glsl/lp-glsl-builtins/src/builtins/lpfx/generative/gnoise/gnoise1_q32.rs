//! 1D Gradient Noise function.
//!
//! Uses random values at grid cell corners and interpolates between them using cubic smoothing.

use crate::builtins::lpfx::generative::random::random1_q32::lpfx_random1;
use crate::glsl::q32::fns::{cubic_q32, mix_q32};
use crate::glsl::q32::types::q32::Q32;

/// 1D Gradient Noise function
///
/// # Arguments
/// * `x` - Input coordinate as Q32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value in [0, 1] range as Q32
#[inline(always)]
pub fn lpfx_gnoise1(x: Q32, seed: u32) -> Q32 {
    // i = floor(x), f = fract(x)
    let i = Q32::from_i32(x.to_i32());
    let f = x - i;

    // Sample corners
    let a = lpfx_random1(i, seed);
    let b = lpfx_random1(i + Q32::ONE, seed);

    // Interpolate using cubic smoothing
    let u = cubic_q32(f);

    // mix(a, b, u)
    mix_q32(a, b, u)
}

/// 1D Gradient Noise function (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate as i32 (Q32 fixed-point)
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value in [0, 1] range as i32 (Q32 fixed-point format)
#[lpfx_impl_macro::lpfx_impl(q32, "float lpfx_gnoise(float x, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_gnoise1_q32(x: i32, seed: u32) -> i32 {
    lpfx_gnoise1(Q32::from_fixed(x), seed).to_fixed()
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    #[test]
    fn test_gnoise1_range() {
        let result = __lpfx_gnoise1_q32(Q32::from_f32(42.5).to_fixed(), 123);
        let val = Q32::from_fixed(result).to_f32();
        assert!(val >= 0.0 && val <= 1.0, "Gnoise should be in [0, 1] range");
    }
}
