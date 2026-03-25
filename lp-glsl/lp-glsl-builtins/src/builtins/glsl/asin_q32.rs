//! Fixed-point 16.16 arcsine function.

use crate::builtins::glsl::atan_q32::__lp_glsl_atan_q32;
use crate::builtins::lpir::fdiv_q32::__lp_lpir_fdiv_q32;
use crate::builtins::lpir::fmul_q32::__lp_lpir_fmul_q32;
use crate::builtins::lpir::fsqrt_q32::__lp_lpir_fsqrt_q32;

/// Fixed-point value of 1.0 (Q16.16 format)
const FIX16_ONE: i32 = 0x00010000; // 65536

/// Compute asin(x) using: asin(x) = atan(x / sqrt(1 - x²))
///
/// Algorithm ported from libfixmath.
/// Domain: |x| <= 1
/// Returns angle in radians in range [-π/2, π/2].
#[unsafe(no_mangle)]
pub extern "C" fn __lp_glsl_asin_q32(x: i32) -> i32 {
    // Domain check: |x| > 1 returns 0 (libfixmath behavior)
    if !(-FIX16_ONE..=FIX16_ONE).contains(&x) {
        return 0;
    }

    // Handle edge cases: asin(1) = π/2, asin(-1) = -π/2
    if x == FIX16_ONE {
        // π/2 in fixed point
        const FIX16_PI: i32 = 205887;
        return FIX16_PI >> 1;
    }
    if x == -FIX16_ONE {
        // -π/2 in fixed point
        const FIX16_PI: i32 = 205887;
        return -(FIX16_PI >> 1);
    }

    // Compute 1 - x²
    let one_minus_x_sq = FIX16_ONE - __lp_lpir_fmul_q32(x, x);

    // Compute sqrt(1 - x²)
    let sqrt_val = __lp_lpir_fsqrt_q32(one_minus_x_sq);

    // Compute x / sqrt(1 - x²)
    let ratio = __lp_lpir_fdiv_q32(x, sqrt_val);

    // Compute atan(ratio)
    __lp_glsl_atan_q32(ratio)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::test_q32_function_relative;

    #[test]
    fn test_asin_basic() {
        let tests = [
            (0.0, 0.0),
            (1.0, 1.5707963267948966),   // asin(1) = π/2
            (-1.0, -1.5707963267948966), // asin(-1) = -π/2
            (0.5, 0.5235987755982989),   // asin(0.5) = π/6
        ];

        // Use 3% tolerance for trig functions
        test_q32_function_relative(|x| __lp_glsl_asin_q32(x), &tests, 0.03, 0.01);
    }
}
