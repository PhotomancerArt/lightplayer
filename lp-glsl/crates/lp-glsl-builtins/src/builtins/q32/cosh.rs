//! Fixed-point 16.16 hyperbolic cosine function.

use super::exp::__lp_q32_exp;
use crate::builtins::q32::div::__lp_q32_div;

/// Fixed-point value of 2.0 (Q16.16 format)
const FIX16_TWO: i32 = 0x00020000; // 131072

/// Compute cosh(x) using: cosh(x) = (exp(x) + exp(-x)) / 2
///
/// Uses the mathematical definition with exp.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_cosh(x: i32) -> i32 {
    // Handle zero case: cosh(0) = 1
    if x == 0 {
        return 0x00010000; // 1.0 in fixed point
    }

    // Compute exp(x) and exp(-x)
    let exp_x = __lp_q32_exp(x);
    let exp_neg_x = __lp_q32_exp(-x);

    // cosh(x) = (exp(x) + exp(-x)) / 2
    let numerator = exp_x + exp_neg_x;
    __lp_q32_div(numerator, FIX16_TWO)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::test_q32_function_relative;

    #[test]
    fn test_cosh_basic() {
        let tests = [
            (0.0, 1.0),
            (1.0, 1.5430806348152437),  // cosh(1)
            (-1.0, 1.5430806348152437), // cosh(-1) = cosh(1)
            (0.5, 1.1276259652063807),  // cosh(0.5)
        ];

        // Use 5% tolerance for hyperbolic functions (uses exp internally)
        test_q32_function_relative(|x| __lp_q32_cosh(x), &tests, 0.05, 0.01);
    }
}
