//! Fixed-point 16.16 base-2 exponential function.

use crate::builtins::glsl::exp_q32::__lps_exp_q32;
use crate::builtins::lpir::fmul_q32::__lp_lpir_fmul_q32;

/// Fixed-point value of ln(2) ≈ 0.693147 (Q16.16 format)
/// ln(2) ≈ 0.6931471805599453
const FIX16_LN2: i32 = 45426; // 0.693147 * 65536 ≈ 45426

/// Compute exp2(x) = 2^x using exp2(x) = exp(x * ln(2))
///
/// This is simpler than porting fr_math's radix-based approach.
/// exp2(x) = 2^x = e^(x * ln(2))
#[unsafe(no_mangle)]
pub extern "C" fn __lps_exp2_q32(x: i32) -> i32 {
    // Compute x * ln(2)
    let x_times_ln2 = __lp_lpir_fmul_q32(x, FIX16_LN2);

    // Compute exp(x * ln(2)) = 2^x
    __lps_exp_q32(x_times_ln2)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::test_q32_function_relative;

    #[test]
    fn test_exp2_basic() {
        let tests = [
            (0.0, 1.0),
            (1.0, 2.0),                // 2^1 = 2
            (2.0, 4.0),                // 2^2 = 4
            (-1.0, 0.5),               // 2^-1 = 0.5
            (0.5, 1.4142135623730951), // 2^0.5 = sqrt(2)
        ];

        // Use 5% tolerance for exp2 (uses exp internally, so accumulates error)
        test_q32_function_relative(|x| __lps_exp2_q32(x), &tests, 0.05, 0.01);
    }
}
