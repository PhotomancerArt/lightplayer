//! Fixed-point 16.16 inverse hyperbolic cosine function.

use super::log::__lp_q32_log;
use super::sqrt::__lp_q32_sqrt;
use crate::builtins::q32::mul::__lp_q32_mul;

/// Fixed-point value of 1.0 (Q16.16 format)
const FIX16_ONE: i32 = 0x00010000; // 65536

/// Compute acosh(x) using: acosh(x) = log(x + sqrt(x² - 1)) for x >= 1
///
/// Uses the mathematical definition with log and sqrt.
/// Domain: x >= 1
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_acosh(x: i32) -> i32 {
    // Domain check: x < 1 returns 0 (invalid domain)
    if x < FIX16_ONE {
        return 0;
    }

    // Handle x = 1: acosh(1) = 0
    if x == FIX16_ONE {
        return 0;
    }

    // Compute x² - 1
    let x_sq = __lp_q32_mul(x, x);
    let x_sq_minus_one = x_sq - FIX16_ONE;

    // Compute sqrt(x² - 1)
    let sqrt_val = __lp_q32_sqrt(x_sq_minus_one);

    // Compute x + sqrt(x² - 1)
    let sum = x + sqrt_val;

    // Compute log(x + sqrt(x² - 1))
    __lp_q32_log(sum)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::test_q32_function_relative;

    #[test]
    fn test_acosh_basic() {
        let tests = [
            (1.0, 0.0),
            (2.0, 1.3169578969248166), // acosh(2)
            (1.5, 0.9624236501192069), // acosh(1.5)
        ];

        // Use 5% tolerance for inverse hyperbolic functions
        test_q32_function_relative(|x| __lp_q32_acosh(x), &tests, 0.05, 0.01);
    }
}
