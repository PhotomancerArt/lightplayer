//! Fixed-point 16.16 scale by power of 2 function.
//!
//! ldexp(x, exp) = x * 2^exp

/// Compute ldexp(x, exp) = x * 2^exp
///
/// In fixed-point Q16.16, multiplying by 2^exp is equivalent to left-shifting by exp bits.
/// For negative exp, we right-shift.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_ldexp(x: i32, exp: i32) -> i32 {
    if exp == 0 {
        return x;
    }

    if exp > 0 {
        // Left shift: x * 2^exp
        // But we need to be careful about overflow
        // For fixed-point, we shift the whole value left by exp bits
        x.wrapping_shl(exp as u32)
    } else {
        // Right shift: x / 2^(-exp) = x >> (-exp)
        // Use arithmetic shift to preserve sign
        x.wrapping_shr((-exp) as u32)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::{fixed_to_float, float_to_fixed};

    #[test]
    fn test_ldexp_one_one() {
        // ldexp(1.0, 1) = 2.0
        let x = float_to_fixed(1.0);
        let result = __lp_q32_ldexp(x, 1);
        let result_float = fixed_to_float(result);
        assert!(
            (result_float - 2.0).abs() < 0.01,
            "Expected ~2.0, got {}",
            result_float
        );
    }

    #[test]
    fn test_ldexp_one_two() {
        // ldexp(1.0, 2) = 4.0
        let x = float_to_fixed(1.0);
        let result = __lp_q32_ldexp(x, 2);
        let result_float = fixed_to_float(result);
        assert!(
            (result_float - 4.0).abs() < 0.01,
            "Expected ~4.0, got {}",
            result_float
        );
    }

    #[test]
    fn test_ldexp_half_neg_one() {
        // ldexp(0.5, -1) = 0.25
        let x = float_to_fixed(0.5);
        let result = __lp_q32_ldexp(x, -1);
        let result_float = fixed_to_float(result);
        assert!(
            (result_float - 0.25).abs() < 0.01,
            "Expected ~0.25, got {}",
            result_float
        );
    }
}
