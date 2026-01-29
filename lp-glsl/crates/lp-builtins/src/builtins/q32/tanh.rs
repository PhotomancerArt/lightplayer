//! Fixed-point 16.16 hyperbolic tangent function.

use super::cosh::__lp_q32_cosh;
use super::sinh::__lp_q32_sinh;
use crate::builtins::q32::div::__lp_q32_div;

/// Compute tanh(x) using: tanh(x) = sinh(x) / cosh(x)
///
/// Uses the mathematical definition with sinh and cosh.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_tanh(x: i32) -> i32 {
    // Handle zero case: tanh(0) = 0
    if x == 0 {
        return 0;
    }

    // Compute sinh(x) and cosh(x)
    let sinh_x = __lp_q32_sinh(x);
    let cosh_x = __lp_q32_cosh(x);

    // tanh(x) = sinh(x) / cosh(x)
    // cosh(x) is never zero, so division is safe
    __lp_q32_div(sinh_x, cosh_x)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::test_q32_function_relative;

    #[test]
    fn test_tanh_basic() {
        let tests = [
            (0.0, 0.0),
            (1.0, 0.7615941559557649),   // tanh(1)
            (-1.0, -0.7615941559557649), // tanh(-1)
            (0.5, 0.46211715726000974),  // tanh(0.5)
        ];

        // Use 5% tolerance for hyperbolic functions
        test_q32_function_relative(|x| __lp_q32_tanh(x), &tests, 0.05, 0.01);
    }
}
