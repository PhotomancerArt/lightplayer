//! Fixed-point 16.16 modulus function.
//!
//! mod(x, y) = x - y * floor(x / y)

use crate::builtins::q32::div::__lp_q32_div;
use crate::builtins::q32::mul::__lp_q32_mul;

/// Fixed-point modulus: mod(x, y) = x - y * floor(x / y)
///
/// Uses div builtin for division and mul builtin for multiplication.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_mod(x: i32, y: i32) -> i32 {
    // Compute x / y using div builtin
    let div_result = __lp_q32_div(x, y);

    // floor(x / y): In fixed-point Q16.16, floor is just shifting right by 16 then left by 16
    // This truncates the fractional part (rounds toward negative infinity for negative numbers)
    // Use arithmetic shift to preserve sign
    let floored = (div_result >> 16) << 16;

    // y * floor(x / y) using mul builtin
    let y_times_floor = __lp_q32_mul(y, floored);

    // x - y * floor(x / y)
    x.wrapping_sub(y_times_floor)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::{fixed_to_float, float_to_fixed};

    #[test]
    fn test_mod_positive_positive() {
        // mod(7.0, 3.0) = 1.0
        let x = float_to_fixed(7.0);
        let y = float_to_fixed(3.0);
        let result = __lp_q32_mod(x, y);
        let result_float = fixed_to_float(result);
        assert!(
            (result_float - 1.0).abs() < 0.01,
            "Expected ~1.0, got {}",
            result_float
        );
    }

    #[test]
    fn test_mod_exact() {
        // mod(6.0, 3.0) = 0.0
        let x = float_to_fixed(6.0);
        let y = float_to_fixed(3.0);
        let result = __lp_q32_mod(x, y);
        let result_float = fixed_to_float(result);
        assert!(
            result_float.abs() < 0.01,
            "Expected ~0.0, got {}",
            result_float
        );
    }

    #[test]
    fn test_mod_negative_dividend() {
        // mod(-7.0, 3.0) = 2.0
        let x = float_to_fixed(-7.0);
        let y = float_to_fixed(3.0);
        let result = __lp_q32_mod(x, y);
        let result_float = fixed_to_float(result);
        assert!(
            (result_float - 2.0).abs() < 0.01,
            "Expected ~2.0, got {}",
            result_float
        );
    }

    #[test]
    fn test_mod_fractional() {
        // mod(7.5, 2.0) = 1.5
        let x = float_to_fixed(7.5);
        let y = float_to_fixed(2.0);
        let result = __lp_q32_mod(x, y);
        let result_float = fixed_to_float(result);
        assert!(
            (result_float - 1.5).abs() < 0.01,
            "Expected ~1.5, got {}",
            result_float
        );
    }
}
