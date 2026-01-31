//! Fixed-point 16.16 exponential function.

use crate::builtins::q32::div::__lp_q32_div;
use crate::builtins::q32::mul::__lp_q32_mul;

/// Fixed-point value of 1.0 (Q16.16 format)
const FIX16_ONE: i32 = 0x00010000; // 65536
/// Fixed-point value of e (Q16.16 format)
const FIX16_E: i32 = 178145;
/// Maximum value before overflow
const FIX16_MAX_EXP: i32 = 681391; // ~10.4 in fixed point
/// Minimum value before underflow
const FIX16_MIN_EXP: i32 = -772243; // ~-11.8 in fixed point

/// Compute exp(x) using power series: exp(x) = 1 + x + x²/2! + x³/3! + ...
///
/// Algorithm ported from libfixmath.
/// For negative x: exp(-x) = 1/exp(x)
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_exp(x: i32) -> i32 {
    // Handle special cases
    if x == 0 {
        return FIX16_ONE;
    }
    if x == FIX16_ONE {
        return FIX16_E;
    }
    if x >= FIX16_MAX_EXP {
        return i32::MAX; // Saturate to maximum
    }
    if x <= FIX16_MIN_EXP {
        return 0; // Underflow to zero
    }

    // The power-series converges much faster on positive values
    // and exp(-x) = 1/exp(x).
    let neg = x < 0;
    let in_value = if neg { -x } else { x };

    let mut result = in_value + FIX16_ONE;
    let mut term = in_value;

    // Compute power series: term_n+1 = term_n * x / n
    for i in 2..30 {
        // Convert i to fixed point for division
        let i_fixed = i << 16;
        term = __lp_q32_mul(term, __lp_q32_div(in_value, i_fixed));
        result += term;

        // Early termination if term becomes small enough
        if (term < 500) && ((i > 15) || (term < 20)) {
            break;
        }
    }

    // Handle negative x: exp(-x) = 1/exp(x)
    if neg {
        result = __lp_q32_div(FIX16_ONE, result);
    }

    result
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::test_q32_function_relative;

    #[test]
    fn test_exp_basic() {
        let tests = [
            (0.0, 1.0),
            (1.0, 2.718281828459045),    // e
            (-1.0, 0.36787944117144233), // 1/e
            (2.0, 7.38905609893065),     // e²
            (0.5, 1.6487212707001282),   // sqrt(e)
        ];

        // Use 3% tolerance for exponential functions
        test_q32_function_relative(|x| __lp_q32_exp(x), &tests, 0.03, 0.01);
    }
}
