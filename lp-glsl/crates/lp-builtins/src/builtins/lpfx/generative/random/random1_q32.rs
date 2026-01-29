//! 1D Random function using sin-based hash.
//!
//! Returns values in [0, 1] range using fract(sin(x + seed) * 43758.5453)

use crate::builtins::q32::__lp_q32_sin;
use crate::glsl::q32::types::q32::Q32;

/// Random constant multiplier
/// In Q16.16: 43758.5453 * 65536 ≈ 2867801088
const RANDOM_MULT: i64 = 2867801088;

/// 1D Random function
///
/// # Arguments
/// * `x` - Input coordinate as Q32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Random value in [0, 1] range as Q32
#[inline(always)]
pub fn lpfx_random1(x: Q32, seed: u32) -> Q32 {
    // Combine x and seed
    let combined = x.to_fixed().wrapping_add(seed as i32);

    // sin(combined) * 43758.5453
    let sin_val = __lp_q32_sin(combined);
    let multiplied = ((sin_val as i64 * RANDOM_MULT) >> 16) as i32;

    // fract() - get fractional part
    Q32::from_fixed(multiplied).frac()
}

/// 1D Random function (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate as i32 (Q32 fixed-point)
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Random value in [0, 1] range as i32 (Q32 fixed-point format)
#[lpfx_impl_macro::lpfx_impl(q32, "float lpfx_random(float x, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_random1_q32(x: i32, seed: u32) -> i32 {
    lpfx_random1(Q32::from_fixed(x), seed).to_fixed()
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    #[test]
    fn test_random1_basic() {
        let result1 = __lpfx_random1_q32(Q32::from_f32(0.0).to_fixed(), 0);
        let result2 = __lpfx_random1_q32(Q32::from_f32(1.0).to_fixed(), 0);
        let result3 = __lpfx_random1_q32(Q32::from_f32(0.0).to_fixed(), 1);

        // Different inputs should produce different outputs
        assert_ne!(
            result1, result2,
            "Random should differ for different x values"
        );
        assert_ne!(result1, result3, "Random should differ for different seeds");

        // Results should be in [0, 1] range (Q32 format)
        let val1 = Q32::from_fixed(result1).to_f32();
        assert!(
            val1 >= 0.0 && val1 <= 1.0,
            "Random should be in [0, 1] range"
        );
    }

    #[test]
    fn test_random1_deterministic() {
        let result1 = __lpfx_random1_q32(Q32::from_f32(42.0).to_fixed(), 123);
        let result2 = __lpfx_random1_q32(Q32::from_f32(42.0).to_fixed(), 123);

        // Same input and seed should produce same output
        assert_eq!(result1, result2, "Random should be deterministic");
    }
}
