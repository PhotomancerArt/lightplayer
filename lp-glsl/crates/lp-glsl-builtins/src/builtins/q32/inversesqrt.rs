//! Fixed-point 16.16 inverse square root function.
//!
//! inversesqrt(x) = 1 / sqrt(x)

use super::sqrt::__lp_q32_sqrt;
use crate::builtins::q32::div::__lp_q32_div;

/// Fixed-point value of 1.0 (Q16.16 format)
const FIX16_ONE: i32 = 0x00010000; // 65536

/// Compute inversesqrt(x) = 1 / sqrt(x)
///
/// Uses sqrt builtin and div builtin.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_inversesqrt(x: i32) -> i32 {
    // Handle zero and negative inputs
    if x <= 0 {
        // Return maximum value (approximation of infinity)
        return 0x7FFF_FFFF;
    }

    // Compute sqrt(x)
    let sqrt_x = __lp_q32_sqrt(x);

    // Compute 1 / sqrt(x)
    __lp_q32_div(FIX16_ONE, sqrt_x)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::test_q32_function_relative;

    #[test]
    fn test_inversesqrt_basic() {
        let tests = [
            (4.0, 0.5),      // 1/sqrt(4) = 1/2 = 0.5
            (9.0, 0.333333), // 1/sqrt(9) = 1/3 ≈ 0.333333
            (16.0, 0.25),    // 1/sqrt(16) = 1/4 = 0.25
            (1.0, 1.0),      // 1/sqrt(1) = 1
            (0.25, 2.0),     // 1/sqrt(0.25) = 1/0.5 = 2.0
        ];

        // Use 5% tolerance (uses sqrt and div internally)
        test_q32_function_relative(|x| __lp_q32_inversesqrt(x), &tests, 0.05, 0.01);
    }
}
