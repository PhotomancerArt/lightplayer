//! Fixed-point 16.16 natural logarithm function.

use crate::builtins::glsl::exp_q32::__lps_exp_q32;
use crate::builtins::lpir::fdiv_q32::__lp_lpir_fdiv_q32;
use crate::builtins::lpir::fmul_q32::__lp_lpir_fmul_q32;

/// Fixed-point value of 1.0 (Q16.16 format)
const FIX16_ONE: i32 = 0x00010000; // 65536
const FIX16_ZERO: i32 = 0;

/// Compute log(x) using Newton-Raphson method.
///
/// Algorithm ported from libfixmath.
/// Uses iterative refinement: solving e(guess) = x using Newton's method.
#[unsafe(no_mangle)]
pub extern "C" fn __lps_log_q32(x: i32) -> i32 {
    // GLSL: log(x) for x <= 0 is undefined — return 0 (edge-exp-domain).
    if x <= 0 {
        return FIX16_ZERO;
    }

    // Special case: log(1) = 0
    if x == FIX16_ONE {
        return 0;
    }

    let mut guess = 2 << 16; // Start with guess = 2.0 (libfixmath uses fix16_from_int(2))
    let mut in_value = x;
    let mut scaling = 0i32;

    // Bring the value to the most accurate range (1 < x < 100)
    // Using e^4 ≈ 54.6, so dividing/multiplying by e^4 adjusts scaling by 4
    const E_TO_FOURTH: i32 = 3578144; // e^4 in fixed point (approximately)

    while in_value > (100 << 16) {
        in_value = __lp_lpir_fdiv_q32(in_value, E_TO_FOURTH);
        scaling += 4;
    }

    while in_value < FIX16_ONE {
        let prev_value = in_value;
        in_value = __lp_lpir_fmul_q32(in_value, E_TO_FOURTH);
        scaling -= 4;

        // Safety check: if multiplication didn't change the value, we're stuck
        // This can happen if the value underflows to 0 or saturates incorrectly
        if in_value == prev_value {
            // Value didn't change - break to avoid infinite loop
            // This indicates underflow or saturation issue
            break;
        }

        // Additional safety: if scaling becomes very negative, we've scaled too far
        // This shouldn't happen in normal cases, but protects against edge cases
        if scaling < -100 {
            break;
        }
    }

    // Newton-Raphson iteration: solving e(guess) = in_value
    // f(guess) = e(guess) - in_value
    // f'(guess) = e(guess)
    // delta = (in_value - e(guess)) / e(guess) = in_value/e(guess) - 1
    let mut count = 0;
    loop {
        let e_guess = __lps_exp_q32(guess);
        let delta = __lp_lpir_fdiv_q32(in_value - e_guess, e_guess);

        // It's unlikely that logarithm is very large, so avoid overshooting.
        // libfixmath clamps to fix16_from_int(3) which is 3 << 16
        let delta_clamped = delta.clamp(-(3 << 16), 3 << 16);

        guess += delta_clamped;

        count += 1;
        // Stop if delta is small enough (within 1) or we've done enough iterations
        // libfixmath checks: (delta > 1) || (delta < -1)
        if count >= 10 || (-1..=1).contains(&delta_clamped) {
            break;
        }
    }

    // Add scaling factor: log(x * e^n) = log(x) + n
    // libfixmath uses fix16_from_int(scaling) which is scaling << 16
    guess + (scaling << 16)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::test_q32_function_relative;

    #[test]
    fn test_log_basic() {
        let tests = [
            (1.0, 0.0),
            (2.718281828459045, 1.0),    // log(e) = 1
            (7.38905609893065, 2.0),     // log(e²) = 2
            (0.36787944117144233, -1.0), // log(1/e) = -1
            (0.5, -0.6931471805599453),  // log(0.5)
        ];

        // Use 5% tolerance for log functions (Newton-Raphson can have some error)
        test_q32_function_relative(|x| __lps_log_q32(x), &tests, 0.05, 0.01);
    }
}
