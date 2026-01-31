//! Fixed-point 16.16 sine function.

use crate::builtins::q32::mul::__lp_q32_mul;

/// Fixed-point value of π (Q16.16 format)
const FIX16_PI: i32 = 205887;

/// Compute sine using Taylor series approximation.
///
/// Algorithm ported from libfixmath's accurate Taylor series implementation.
/// Accuracy: ~2.1%
///
/// Formula: sin(x) ≈ x - x³/6 + x⁵/120 - x⁷/5040 + x⁹/362880 - x¹¹/39916800
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_sin(x: i32) -> i32 {
    // Handle zero case early (sin(0) = 0 exactly)
    if x == 0 {
        return 0;
    }

    // Range reduction: reduce to [-2π, 2π]
    let two_pi = FIX16_PI << 1;
    let mut temp_angle = x % two_pi;

    // Further reduce to [-π, π]
    if temp_angle > FIX16_PI {
        temp_angle -= two_pi;
    } else if temp_angle < -FIX16_PI {
        temp_angle += two_pi;
    }

    // Compute temp_angle² for Taylor series
    let temp_angle_sq = __lp_q32_mul(temp_angle, temp_angle);

    // Taylor series: x - x³/6 + x⁵/120 - x⁷/5040 + x⁹/362880 - x¹¹/39916800
    let mut result = temp_angle;

    // x³ term: -x³/6
    let mut term = __lp_q32_mul(temp_angle, temp_angle_sq);
    result -= term / 6;

    // x⁵ term: +x⁵/120
    term = __lp_q32_mul(term, temp_angle_sq);
    result += term / 120;

    // x⁷ term: -x⁷/5040
    term = __lp_q32_mul(term, temp_angle_sq);
    result -= term / 5040;

    // x⁹ term: +x⁹/362880
    term = __lp_q32_mul(term, temp_angle_sq);
    result += term / 362880;

    // x¹¹ term: -x¹¹/39916800
    term = __lp_q32_mul(term, temp_angle_sq);
    result -= term / 39916800;

    result
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::test_q32_function_relative;

    #[test]
    fn test_sin_basic() {
        let tests = [
            (0.0, 0.0),
            (1.5707963267948966, 1.0),   // π/2
            (3.141592653589793, 0.0),    // π
            (-1.5707963267948966, -1.0), // -π/2
        ];

        // Use 3% tolerance for trig functions (~2.1% accuracy)
        test_q32_function_relative(|x| __lp_q32_sin(x), &tests, 0.03, 0.01);
    }

    #[test]
    fn test_sin_range_reduction() {
        let tests = [
            (6.283185307179586, 0.0),  // 2π
            (9.42477796076938, 0.0),   // 3π
            (-6.283185307179586, 0.0), // -2π
        ];

        test_q32_function_relative(|x| __lp_q32_sin(x), &tests, 0.03, 0.01);
    }

    #[test]
    fn test_sin_small_angles() {
        let tests = [
            (0.1, 0.09983341664682815),
            (0.5, 0.479425538604203),
            (-0.1, -0.09983341664682815),
        ];

        test_q32_function_relative(|x| __lp_q32_sin(x), &tests, 0.03, 0.01);
    }
}
