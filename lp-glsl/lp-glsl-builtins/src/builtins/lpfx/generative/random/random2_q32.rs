//! 2D Random function using sin-based hash.
//!
//! Returns values in [0, 1] range using fract(sin(dot(p, vec2(12.9898, 78.233)) + seed) * 43758.5453)

use crate::builtins::q32::__lp_q32_sin;
use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec2_q32::Vec2Q32;

/// Random constant multiplier
/// In Q16.16: 43758.5453 * 65536 ≈ 2867801088
const RANDOM_MULT: i64 = 2867801088;

/// Dot product constants for 2D random
/// vec2(12.9898, 78.233) in Q16.16
const DOT_X: i32 = 851456; // 12.9898 * 65536
const DOT_Y: i32 = 5126144; // 78.233 * 65536

/// 2D Random function
///
/// # Arguments
/// * `p` - Input coordinates as Vec2Q32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Random value in [0, 1] range as Q32
#[inline(always)]
pub fn lpfx_random2(p: Vec2Q32, seed: u32) -> Q32 {
    // dot(p, vec2(12.9898, 78.233))
    let dot_result = ((p.x.to_fixed() as i64 * DOT_X as i64) >> 16) as i32
        + ((p.y.to_fixed() as i64 * DOT_Y as i64) >> 16) as i32;

    // Combine with seed
    let combined = dot_result.wrapping_add(seed as i32);

    // sin(combined) * 43758.5453
    let sin_val = __lp_q32_sin(combined);
    let multiplied = ((sin_val as i64 * RANDOM_MULT) >> 16) as i32;

    // fract() - get fractional part
    Q32::from_fixed(multiplied).frac()
}

/// 2D Random function (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate as i32 (Q32 fixed-point)
/// * `y` - Y coordinate as i32 (Q32 fixed-point)
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Random value in [0, 1] range as i32 (Q32 fixed-point format)
#[lpfx_impl_macro::lpfx_impl(q32, "float lpfx_random(vec2 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_random2_q32(x: i32, y: i32, seed: u32) -> i32 {
    lpfx_random2(Vec2Q32::new(Q32::from_fixed(x), Q32::from_fixed(y)), seed).to_fixed()
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    #[test]
    fn test_random2_basic() {
        let result1 = __lpfx_random2_q32(
            Q32::from_f32(0.0).to_fixed(),
            Q32::from_f32(0.0).to_fixed(),
            0,
        );
        let result2 = __lpfx_random2_q32(
            Q32::from_f32(1.0).to_fixed(),
            Q32::from_f32(0.0).to_fixed(),
            0,
        );
        let result3 = __lpfx_random2_q32(
            Q32::from_f32(0.0).to_fixed(),
            Q32::from_f32(0.0).to_fixed(),
            1,
        );

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
    fn test_random2_deterministic() {
        let result1 = __lpfx_random2_q32(
            Q32::from_f32(42.0).to_fixed(),
            Q32::from_f32(10.0).to_fixed(),
            123,
        );
        let result2 = __lpfx_random2_q32(
            Q32::from_f32(42.0).to_fixed(),
            Q32::from_f32(10.0).to_fixed(),
            123,
        );

        // Same input and seed should produce same output
        assert_eq!(result1, result2, "Random should be deterministic");
    }
}
