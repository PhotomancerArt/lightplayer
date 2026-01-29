//! Fixed-point 16.16 division.

const MAX_FIXED: i32 = 0x7FFF_FFFF; // Maximum representable fixed-point value
const MIN_FIXED: i32 = i32::MIN; // Minimum representable fixed-point value

/// Fixed-point division: dividend / divisor
///
/// Uses native Rust division. Handles division by zero by saturating to max/min fixed-point values.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_div(dividend: i32, divisor: i32) -> i32 {
    // Handle division by zero: saturate to max/min based on sign
    if divisor == 0 {
        if dividend >= 0 {
            return MAX_FIXED;
        } else {
            return MIN_FIXED;
        }
    }

    // Use native Rust division with i64 to avoid overflow
    // Convert to i64, multiply by scale to maintain precision, divide, then scale back
    let dividend_wide = dividend as i64;
    let divisor_wide = divisor as i64;

    // Fixed-point division: (dividend * scale) / divisor
    // This gives us the result in fixed-point format
    let result_wide = (dividend_wide << 16) / divisor_wide;

    // Saturate to i32 range
    if result_wide > MAX_FIXED as i64 {
        MAX_FIXED
    } else if result_wide < MIN_FIXED as i64 {
        MIN_FIXED
    } else {
        result_wide as i32
    }
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::{fixed_to_float, float_to_fixed};

    #[test]
    fn test_basic_division() {
        let tests = [
            (10.0, 2.0, 5.0),
            (15.0, 3.0, 5.0),
            (20.0, 2.0, 10.0),
            (7.5, 1.0, 7.5),
        ];

        for (dividend, divisor, expected) in tests {
            let dividend_fixed = float_to_fixed(dividend);
            let divisor_fixed = float_to_fixed(divisor);
            let result_fixed = __lp_q32_div(dividend_fixed, divisor_fixed);
            let result = fixed_to_float(result_fixed);

            std::println!(
                "Test: {} / {} -> Expected: {}, Actual: {}",
                dividend,
                divisor,
                expected,
                result
            );

            assert!(
                (result - expected).abs() < 0.01,
                "Test failed: {} / {}; actual: {}; expected {}",
                dividend,
                divisor,
                result,
                expected
            );
        }
    }

    #[test]
    fn test_division_by_zero() {
        // Division by zero should saturate to max/min based on sign
        let pos_dividend = float_to_fixed(10.0);
        let neg_dividend = float_to_fixed(-10.0);
        let zero = 0;

        let result_pos = __lp_q32_div(pos_dividend, zero);
        let result_neg = __lp_q32_div(neg_dividend, zero);

        assert_eq!(
            result_pos, MAX_FIXED,
            "Positive / 0 should saturate to MAX_FIXED"
        );
        assert_eq!(
            result_neg, MIN_FIXED,
            "Negative / 0 should saturate to MIN_FIXED"
        );
    }

    #[test]
    fn test_sign_handling() {
        let tests = [
            (10.0, 2.0, 5.0),
            (10.0, -2.0, -5.0),
            (-10.0, 2.0, -5.0),
            (-10.0, -2.0, 5.0),
        ];

        for (dividend, divisor, expected) in tests {
            let dividend_fixed = float_to_fixed(dividend);
            let divisor_fixed = float_to_fixed(divisor);
            let result_fixed = __lp_q32_div(dividend_fixed, divisor_fixed);
            let result = fixed_to_float(result_fixed);

            std::println!(
                "Test: {} / {} -> Expected: {}, Actual: {}",
                dividend,
                divisor,
                expected,
                result
            );

            assert!(
                (result - expected).abs() < 0.01,
                "Test failed: {} / {}; actual: {}; expected {}",
                dividend,
                divisor,
                result,
                expected
            );
        }
    }

    #[test]
    fn test_edge_cases() {
        // Test values near boundaries
        let max_val = MAX_FIXED;
        let min_val = MIN_FIXED;
        let one = float_to_fixed(1.0);

        // MAX / 1 should be MAX
        let result_max = __lp_q32_div(max_val, one);
        assert!(
            result_max >= MAX_FIXED - 1000,
            "MAX / 1 should be close to MAX"
        );

        // MIN / 1 should be MIN
        let result_min = __lp_q32_div(min_val, one);
        assert!(
            result_min <= MIN_FIXED + 1000,
            "MIN / 1 should be close to MIN"
        );
    }

    #[test]
    fn test_small_divisors() {
        // Test small divisors (< 2^16) - this was a bug fix in inline code
        // The builtin should handle this correctly with (dividend << 16) / divisor
        let tests = [
            (10.0, 0.1, 100.0),
            (5.0, 0.5, 10.0),
            (1.0, 0.001, 1000.0),
            (100.0, 0.01, 10000.0),
        ];

        for (dividend, divisor, expected) in tests {
            let dividend_fixed = float_to_fixed(dividend);
            let divisor_fixed = float_to_fixed(divisor);
            let result_fixed = __lp_q32_div(dividend_fixed, divisor_fixed);
            let result = fixed_to_float(result_fixed);

            std::println!(
                "Test: {} / {} -> Expected: {}, Actual: {}",
                dividend,
                divisor,
                expected,
                result
            );

            // Use larger tolerance for small divisors due to precision limits
            let tolerance = if divisor < 1.0 { 10.0 } else { 0.01 };
            assert!(
                (result - expected).abs() < tolerance,
                "Test failed: {} / {}; actual: {}; expected {}",
                dividend,
                divisor,
                result,
                expected
            );
        }
    }
}
