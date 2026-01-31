//! Fixed-point 16.16 inverse hyperbolic sine function.

use super::log::__lp_q32_log;
use super::sqrt::__lp_q32_sqrt;
use crate::builtins::q32::mul::__lp_q32_mul;

/// Fixed-point value of 1.0 (Q16.16 format)
const FIX16_ONE: i32 = 0x00010000; // 65536

/// Compute asinh(x) using: asinh(x) = log(x + sqrt(x² + 1))
///
/// Uses the mathematical definition with log and sqrt.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_asinh(x: i32) -> i32 {
    // Handle zero case: asinh(0) = 0
    if x == 0 {
        return 0;
    }

    // Compute x² + 1
    let x_sq = __lp_q32_mul(x, x);
    let x_sq_plus_one = x_sq + FIX16_ONE;

    // Compute sqrt(x² + 1)
    let sqrt_val = __lp_q32_sqrt(x_sq_plus_one);

    // Compute x + sqrt(x² + 1) (fixed-point addition is just integer addition)
    let sum = x + sqrt_val;

    // Compute log(x + sqrt(x² + 1))
    __lp_q32_log(sum)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::test_q32_function_relative;

    #[test]
    fn test_asinh_basic() {
        let tests = [
            (0.0, 0.0),
            (1.0, 0.881373587019543),   // asinh(1)
            (-1.0, -0.881373587019543), // asinh(-1)
            (0.5, 0.48121182505960347), // asinh(0.5)
        ];

        // Use 5% tolerance for inverse hyperbolic functions
        test_q32_function_relative(|x| __lp_q32_asinh(x), &tests, 0.05, 0.01);
    }
}
