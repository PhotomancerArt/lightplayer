//! Fixed-point 16.16 inverse hyperbolic tangent function.

use super::log::__lp_q32_log;
use crate::builtins::q32::div::__lp_q32_div;

/// Fixed-point value of 1.0 (Q16.16 format)
const FIX16_ONE: i32 = 0x00010000; // 65536

/// Compute atanh(x) using: atanh(x) = (1/2) * log((1+x)/(1-x)) for |x| < 1
///
/// Uses the mathematical definition with log and division.
/// Domain: |x| < 1
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_atanh(x: i32) -> i32 {
    // Handle zero case: atanh(0) = 0
    if x == 0 {
        return 0;
    }

    // Domain check: |x| >= 1 returns 0 (invalid domain)
    if x >= FIX16_ONE || x <= -FIX16_ONE {
        return 0;
    }

    // Compute (1 + x) / (1 - x)
    let one_plus_x = FIX16_ONE + x;
    let one_minus_x = FIX16_ONE - x;
    let ratio = __lp_q32_div(one_plus_x, one_minus_x);

    // Compute log((1+x)/(1-x))
    let log_val = __lp_q32_log(ratio);

    // Compute (1/2) * log((1+x)/(1-x))
    // Multiply by 0.5 in fixed point: multiply by 32768 then shift right by 16
    // Or simpler: divide by 2 in fixed point = divide by 131072
    const FIX16_TWO: i32 = 0x00020000; // 131072
    __lp_q32_div(log_val, FIX16_TWO)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::test_q32_function_relative;

    #[test]
    fn test_atanh_basic() {
        let tests = [
            (0.0, 0.0),
            (0.5, 0.5493061443340549),   // atanh(0.5)
            (-0.5, -0.5493061443340549), // atanh(-0.5)
            (0.9, 1.4722194895832204),   // atanh(0.9)
        ];

        // Use 5% tolerance for inverse hyperbolic functions
        test_q32_function_relative(|x| __lp_q32_atanh(x), &tests, 0.05, 0.01);
    }
}
