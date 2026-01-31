//! Fixed-point 16.16 hyperbolic sine function.

use super::exp::__lp_q32_exp;
use crate::builtins::q32::div::__lp_q32_div;

/// Fixed-point value of 2.0 (Q16.16 format)
const FIX16_TWO: i32 = 0x00020000; // 131072

/// Compute sinh(x) using: sinh(x) = (exp(x) - exp(-x)) / 2
///
/// Uses the mathematical definition with exp.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_sinh(x: i32) -> i32 {
    // Handle zero case
    if x == 0 {
        return 0;
    }

    // Compute exp(x) and exp(-x)
    let exp_x = __lp_q32_exp(x);
    let exp_neg_x = __lp_q32_exp(-x);

    // sinh(x) = (exp(x) - exp(-x)) / 2
    let numerator = exp_x - exp_neg_x;
    __lp_q32_div(numerator, FIX16_TWO)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::test_q32_function_relative;

    #[test]
    fn test_sinh_basic() {
        let tests = [
            (0.0, 0.0),
            (1.0, 1.1752011936438014),   // sinh(1)
            (-1.0, -1.1752011936438014), // sinh(-1)
            (0.5, 0.5210953054937474),   // sinh(0.5)
        ];

        // Use 5% tolerance for hyperbolic functions (uses exp internally)
        test_q32_function_relative(|x| __lp_q32_sinh(x), &tests, 0.05, 0.01);
    }
}
