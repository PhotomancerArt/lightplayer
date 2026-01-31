//! Fixed-point 16.16 fused multiply-add function.
//!
//! fma(a, b, c) = a * b + c

use crate::builtins::q32::mul::__lp_q32_mul;

/// Compute fma(a, b, c) = a * b + c
///
/// Uses mul builtin for multiplication, then adds c.
/// In fixed-point, we can't truly fuse the operations, but we use mul builtin
/// for better precision than doing the operations separately.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_fma(a: i32, b: i32, c: i32) -> i32 {
    // Compute a * b using mul builtin
    let product = __lp_q32_mul(a, b);

    // Add c (simple addition in fixed-point)
    product.wrapping_add(c)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::{fixed_to_float, float_to_fixed};

    #[test]
    fn test_fma_simple() {
        // fma(2.0, 3.0, 4.0) = 10.0
        let a = float_to_fixed(2.0);
        let b = float_to_fixed(3.0);
        let c = float_to_fixed(4.0);
        let result = __lp_q32_fma(a, b, c);
        let result_float = fixed_to_float(result);
        assert!(
            (result_float - 10.0).abs() < 0.01,
            "Expected ~10.0, got {}",
            result_float
        );
    }

    #[test]
    fn test_fma_negative() {
        // fma(2.0, -3.0, 5.0) = -1.0
        let a = float_to_fixed(2.0);
        let b = float_to_fixed(-3.0);
        let c = float_to_fixed(5.0);
        let result = __lp_q32_fma(a, b, c);
        let result_float = fixed_to_float(result);
        assert!(
            (result_float - (-1.0)).abs() < 0.01,
            "Expected ~-1.0, got {}",
            result_float
        );
    }

    #[test]
    fn test_fma_fractions() {
        // fma(1.5, 2.0, 0.5) = 3.5
        let a = float_to_fixed(1.5);
        let b = float_to_fixed(2.0);
        let c = float_to_fixed(0.5);
        let result = __lp_q32_fma(a, b, c);
        let result_float = fixed_to_float(result);
        assert!(
            (result_float - 3.5).abs() < 0.01,
            "Expected ~3.5, got {}",
            result_float
        );
    }
}
