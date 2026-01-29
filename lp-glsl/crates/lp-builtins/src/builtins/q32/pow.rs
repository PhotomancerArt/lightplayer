//! Fixed-point 16.16 power function.

use super::exp2::__lp_q32_exp2;
use super::log2::__lp_q32_log2;
use crate::builtins::q32::div::__lp_q32_div;
use crate::builtins::q32::mul::__lp_q32_mul;

/// Fixed-point value of 1.0 (Q16.16 format)
const FIX16_ONE: i32 = 0x00010000; // 65536
/// Fixed-point value of 0.0 (Q16.16 format)
const FIX16_ZERO: i32 = 0;

/// Compute pow(x, y) = x^y
///
/// Algorithm ported from fpm library.
/// For fractional exponents: pow(x, y) = exp2(log2(x) * y)
/// For integer exponents: uses iterative multiplication for efficiency
/// Special cases:
/// - pow(x, 0) = 1
/// - pow(0, y) = 0 for y > 0
/// - pow(x, -y) = 1 / pow(x, y)
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_pow(x: i32, y: i32) -> i32 {
    // Special case: pow(x, 0) = 1
    if y == 0 {
        return FIX16_ONE;
    }

    // Special case: pow(0, y) = 0 for y > 0
    if x == FIX16_ZERO {
        // For y > 0, return 0; for y < 0, this would be infinity/undefined, return 0
        return FIX16_ZERO;
    }

    // Handle negative exponent: pow(x, -y) = 1 / pow(x, y)
    if y < 0 {
        let result = __lp_q32_pow(x, -y);
        return __lp_q32_div(FIX16_ONE, result);
    }

    // Check if exponent is an integer (no fractional part)
    // In fixed point Q16.16, integer values have lower 16 bits = 0
    if (y & 0xFFFF) == 0 {
        // Integer exponent: use iterative multiplication
        let exp_int = y >> 16; // Extract integer part
        if exp_int == 0 {
            return FIX16_ONE;
        }
        if exp_int == 1 {
            return x;
        }

        // Compute x^exp_int by repeated multiplication
        let mut result = FIX16_ONE;
        let mut base = x;
        let mut exp = exp_int;

        while exp > 0 {
            if exp & 1 != 0 {
                result = __lp_q32_mul(result, base);
            }
            base = __lp_q32_mul(base, base);
            exp >>= 1;
        }

        return result;
    }

    // For negative bases, we don't support fractional exponents
    // (would require complex numbers for some cases)
    if x < 0 {
        // Return 0 for invalid case (could also return error, but 0 is safer)
        return FIX16_ZERO;
    }

    // Fractional exponent: pow(x, y) = exp2(log2(x) * y)
    let log2_x = __lp_q32_log2(x);
    let product = __lp_q32_mul(log2_x, y);
    __lp_q32_exp2(product)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    /// Test pow with 2-arg function
    fn test_pow_helper(inputs: &[(f32, f32, f32)], tolerance: f32, min_tolerance: f32) {
        for (x, y, expected) in inputs {
            let x_fixed = crate::util::test_helpers::float_to_fixed(*x);
            let y_fixed = crate::util::test_helpers::float_to_fixed(*y);
            let result_fixed = __lp_q32_pow(x_fixed, y_fixed);
            let result_float = crate::util::test_helpers::fixed_to_float(result_fixed);

            let abs_error = (result_float - expected).abs();
            let rel_tolerance = expected.abs() * tolerance;
            let effective_tolerance = rel_tolerance.max(min_tolerance);

            std::println!(
                "Test: pow({}, {}) -> Expected: {}, Actual: {}, Error: {}, Tolerance: {}",
                x,
                y,
                expected,
                result_float,
                abs_error,
                effective_tolerance
            );

            assert!(
                abs_error < effective_tolerance,
                "Test failed: pow({}, {}); expected {}, got {} (error: {}, tolerance: {})",
                x,
                y,
                expected,
                result_float,
                abs_error,
                effective_tolerance
            );
        }
    }

    #[test]
    fn test_pow_basic() {
        let tests = [
            (2.0, 2.0, 4.0),                // 2^2 = 4
            (2.0, 3.0, 8.0),                // 2^3 = 8
            (2.0, 0.5, 1.4142135623730951), // 2^0.5 = sqrt(2)
            (3.0, 2.0, 9.0),                // 3^2 = 9
            (4.0, 0.5, 2.0),                // 4^0.5 = 2
            (1.0, 5.0, 1.0),                // 1^5 = 1
        ];

        // Use 5% tolerance for pow (uses exp2/log2 internally)
        test_pow_helper(&tests, 0.05, 0.01);
    }

    #[test]
    fn test_pow_special_cases() {
        let tests = [
            (2.0, 0.0, 1.0), // pow(x, 0) = 1
            (0.0, 2.0, 0.0), // pow(0, y>0) = 0
            (5.0, 1.0, 5.0), // pow(x, 1) = x
        ];

        test_pow_helper(&tests, 0.05, 0.01);
    }
}
