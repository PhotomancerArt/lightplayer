//! Fixed-point 16.16 hyperbolic sine function.

use crate::builtins::glsl::exp_q32::__lps_exp_q32;

/// Compute sinh(x) using: sinh(x) = (exp(x) - exp(-x)) / 2
///
/// Uses the mathematical definition with exp.
#[unsafe(no_mangle)]
pub extern "C" fn __lps_sinh_q32(x: i32) -> i32 {
    // Handle zero case
    if x == 0 {
        return 0;
    }

    // Compute exp(x) and exp(-x)
    let exp_x = __lps_exp_q32(x);
    let exp_neg_x = __lps_exp_q32(x.saturating_neg());

    // sinh(x) = (exp(x) - exp(-x)) / 2
    half_i64_to_i32(exp_x as i64 - exp_neg_x as i64)
}

fn half_i64_to_i32(value: i64) -> i32 {
    (value / 2).clamp(i32::MIN as i64, i32::MAX as i64) as i32
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
        test_q32_function_relative(|x| __lps_sinh_q32(x), &tests, 0.05, 0.01);
    }

    #[test]
    fn test_sinh_extreme_inputs_do_not_panic() {
        assert!(__lps_sinh_q32(i32::MAX) >= 0);
        assert!(__lps_sinh_q32(i32::MIN) <= 0);
    }
}
