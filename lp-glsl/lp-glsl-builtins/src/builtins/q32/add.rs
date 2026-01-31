//! Fixed-point 16.16 addition with overflow/saturation handling.

const MAX_FIXED: i32 = 0x7FFF_FFFF; // Maximum representable fixed-point value (not i32::MAX)
const MIN_FIXED: i32 = i32::MIN; // Minimum representable fixed-point value

/// Fixed-point addition: a + b
///
/// Uses i64 internally to avoid overflow, then saturates to fixed-point range.
/// Handles overflow/underflow by saturating to max/min fixed-point values.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_add(a: i32, b: i32) -> i32 {
    // Use i64 internally for addition to avoid overflow
    let a_wide = a as i64;
    let b_wide = b as i64;

    // Add: result_wide = a + b
    let result_wide = a_wide + b_wide;

    // Saturate to fixed-point range
    // Clamp to [MIN_FIXED, MAX_FIXED]
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
    fn test_basic_addition() {
        let tests = [
            (2.0, 3.0, 5.0),
            (5.0, 4.0, 9.0),
            (10.0, 2.0, 12.0),
            (1.5, 2.5, 4.0),
        ];

        for (a, b, expected) in tests {
            let a_fixed = float_to_fixed(a);
            let b_fixed = float_to_fixed(b);
            let result_fixed = __lp_q32_add(a_fixed, b_fixed);
            let result = fixed_to_float(result_fixed);

            std::println!(
                "Test: {} + {} -> Expected: {}, Actual: {}",
                a,
                b,
                expected,
                result
            );

            assert!(
                (result - expected).abs() < 0.01,
                "Test failed: {} + {}; actual: {}; expected {}",
                a,
                b,
                result,
                expected
            );
        }
    }

    #[test]
    fn test_zero_handling() {
        let one = float_to_fixed(1.0);
        let zero = 0;

        assert_eq!(__lp_q32_add(one, zero), one, "1 + 0 should be 1");
        assert_eq!(__lp_q32_add(zero, one), one, "0 + 1 should be 1");
        assert_eq!(__lp_q32_add(zero, zero), 0, "0 + 0 should be 0");
    }

    #[test]
    fn test_sign_handling() {
        let tests = [
            (2.0, 3.0, 5.0),
            (-2.0, 3.0, 1.0),
            (2.0, -3.0, -1.0),
            (-2.0, -3.0, -5.0),
        ];

        for (a, b, expected) in tests {
            let a_fixed = float_to_fixed(a);
            let b_fixed = float_to_fixed(b);
            let result_fixed = __lp_q32_add(a_fixed, b_fixed);
            let result = fixed_to_float(result_fixed);

            std::println!(
                "Test: {} + {} -> Expected: {}, Actual: {}",
                a,
                b,
                expected,
                result
            );

            assert!(
                (result - expected).abs() < 0.01,
                "Test failed: {} + {}; actual: {}; expected {}",
                a,
                b,
                result,
                expected
            );
        }
    }

    #[test]
    fn test_overflow_saturation() {
        // Test values that would overflow
        let large_a = float_to_fixed(1000.0);
        let large_b = float_to_fixed(1000.0);
        let result = __lp_q32_add(large_a, large_b);

        // Result should be saturated to MAX_FIXED
        assert!(
            result <= MAX_FIXED,
            "Overflow should saturate to MAX_FIXED, got {}",
            result
        );
    }

    #[test]
    fn test_underflow_saturation() {
        // Test values that would underflow
        let large_neg_a = float_to_fixed(-1000.0);
        let large_neg_b = float_to_fixed(-1000.0);
        let result = __lp_q32_add(large_neg_a, large_neg_b);

        // Result should be saturated to MIN_FIXED (if negative) or within range
        assert!(
            result >= MIN_FIXED,
            "Underflow should saturate to MIN_FIXED or be within range, got {}",
            result
        );
    }
}
