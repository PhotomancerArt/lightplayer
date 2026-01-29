//! Fixed-point 16.16 round function.
//!
//! round(x) rounds to nearest integer, rounding halfway cases away from zero

/// Compute round(x) - round to nearest integer, halfway cases away from zero
///
/// In fixed-point Q16.16:
/// - Add 0.5 (32768) for all numbers
/// - Then truncate by shifting right 16 bits and left 16 bits
/// - For negative numbers, adding 0.5 then truncating rounds away from zero correctly
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_round(x: i32) -> i32 {
    if x == 0 {
        return 0;
    }

    // Halfway point: 0x8000 (32768 = 0.5 in Q16.16)
    let half = 0x8000i32;

    if x > 0 {
        // For positive: add 0.5, then truncate
        let added = x.wrapping_add(half);
        (added >> 16) << 16
    } else {
        // For negative: add 0.5, then truncate
        let added = x.wrapping_add(half);
        let result = (added >> 16) << 16;

        // Special case: if we're at exactly halfway (-1.5, -2.5, etc.),
        // adding 0.5 gives us exactly an integer, but we need to round away from zero
        // (more negative). Check if original was at halfway point.
        let fractional_part = x & 0xFFFF;
        if fractional_part == half {
            // At halfway: round away from zero (subtract 1 more)
            result.wrapping_sub(0x10000) // Subtract 1.0 in fixed point
        } else {
            result
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::{fixed_to_float, float_to_fixed};

    #[test]
    fn test_round_positive() {
        // round(1.7) = 2.0
        let x = float_to_fixed(1.7);
        let result = __lp_q32_round(x);
        let result_float = fixed_to_float(result);
        assert!(
            (result_float - 2.0).abs() < 0.01,
            "Expected ~2.0, got {}",
            result_float
        );
    }

    #[test]
    fn test_round_negative() {
        // round(-1.7) = -2.0
        let x = float_to_fixed(-1.7);
        let result = __lp_q32_round(x);
        let result_float = fixed_to_float(result);
        assert!(
            (result_float - (-2.0)).abs() < 0.01,
            "Expected ~-2.0, got {}",
            result_float
        );
    }

    #[test]
    fn test_round_halfway_positive() {
        // round(1.5) = 2.0 (away from zero)
        let x = float_to_fixed(1.5);
        let result = __lp_q32_round(x);
        let result_float = fixed_to_float(result);
        assert!(
            (result_float - 2.0).abs() < 0.01,
            "Expected ~2.0, got {}",
            result_float
        );
    }

    #[test]
    fn test_round_halfway_negative() {
        // round(-1.5) = -2.0 (away from zero)
        let x = float_to_fixed(-1.5);
        let result = __lp_q32_round(x);
        let result_float = fixed_to_float(result);
        assert!(
            (result_float - (-2.0)).abs() < 0.01,
            "Expected ~-2.0, got {}",
            result_float
        );
    }
}
