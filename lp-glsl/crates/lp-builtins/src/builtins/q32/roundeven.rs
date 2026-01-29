//! Fixed-point 16.16 roundeven function.
//!
//! roundeven(x) rounds to nearest integer, rounding halfway cases to nearest even

/// Compute roundeven(x) - round to nearest integer, halfway cases to nearest even
///
/// In fixed-point Q16.16:
/// - Check if we're at a halfway point (fractional part == 0.5)
/// - If halfway and integer part is odd, round away from zero
/// - If halfway and integer part is even, round toward zero
/// - Otherwise, round normally (add/subtract 0.5 and truncate)
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_roundeven(x: i32) -> i32 {
    if x == 0 {
        return 0;
    }

    // Extract integer and fractional parts
    // For negative numbers, right shift rounds toward negative infinity, not toward zero
    // So we need to adjust: add 0xFFFF before shifting to truncate toward zero
    let integer_part = if x >= 0 { x >> 16 } else { (x + 0xFFFF) >> 16 };
    let fractional_part = x & 0xFFFF;

    // Check if we're at halfway point (fractional part == 0x8000 = 0.5)
    let is_halfway = fractional_part == 0x8000;

    if is_halfway {
        // Round to nearest even
        // If integer part is even, round toward zero (keep as is)
        // If integer part is odd, round away from zero
        if integer_part & 1 == 0 {
            // Even: round toward zero (truncate)
            integer_part << 16
        } else {
            // Odd: round away from zero
            if x > 0 {
                (integer_part + 1) << 16
            } else {
                (integer_part - 1) << 16
            }
        }
    } else {
        // Normal rounding: add 0.5 and truncate (works for both positive and negative)
        let half = 0x8000i32;
        let added = x.wrapping_add(half);
        (added >> 16) << 16
    }
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::{fixed_to_float, float_to_fixed};

    #[test]
    fn test_roundeven_halfway_even() {
        // roundeven(2.5) = 2.0 (round to even)
        let x = float_to_fixed(2.5);
        let result = __lp_q32_roundeven(x);
        let result_float = fixed_to_float(result);
        assert!(
            (result_float - 2.0).abs() < 0.01,
            "Expected ~2.0, got {}",
            result_float
        );
    }

    #[test]
    fn test_roundeven_halfway_odd() {
        // roundeven(1.5) = 2.0 (round to even)
        let x = float_to_fixed(1.5);
        let result = __lp_q32_roundeven(x);
        let result_float = fixed_to_float(result);
        assert!(
            (result_float - 2.0).abs() < 0.01,
            "Expected ~2.0, got {}",
            result_float
        );
    }

    #[test]
    fn test_roundeven_negative_halfway_even() {
        // roundeven(-2.5) = -2.0 (round to even)
        let x = float_to_fixed(-2.5);
        let result = __lp_q32_roundeven(x);
        let result_float = fixed_to_float(result);
        assert!(
            (result_float - (-2.0)).abs() < 0.01,
            "Expected ~-2.0, got {}",
            result_float
        );
    }

    #[test]
    fn test_roundeven_normal() {
        // roundeven(1.7) = 2.0 (normal rounding)
        let x = float_to_fixed(1.7);
        let result = __lp_q32_roundeven(x);
        let result_float = fixed_to_float(result);
        assert!(
            (result_float - 2.0).abs() < 0.01,
            "Expected ~2.0, got {}",
            result_float
        );
    }
}
