//! Fixed-point 16.16 arccosine function.

use super::asin::__lp_q32_asin;

/// Fixed-point value of π (Q16.16 format)
const FIX16_PI: i32 = 205887;

/// Compute acos(x) using: acos(x) = π/2 - asin(x)
///
/// Algorithm ported from libfixmath.
/// Domain: |x| <= 1
/// Returns angle in radians in range [0, π].
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_acos(x: i32) -> i32 {
    let half_pi = FIX16_PI >> 1;
    half_pi - __lp_q32_asin(x)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::test_q32_function_relative;

    #[test]
    fn test_acos_basic() {
        let tests = [
            (1.0, 0.0),                // acos(1) = 0
            (0.0, 1.5707963267948966), // acos(0) = π/2
            (-1.0, 3.141592653589793), // acos(-1) = π
            (0.5, 1.0471975511965979), // acos(0.5) = π/3
        ];

        // Use 3% tolerance for trig functions
        test_q32_function_relative(|x| __lp_q32_acos(x), &tests, 0.03, 0.01);
    }
}
