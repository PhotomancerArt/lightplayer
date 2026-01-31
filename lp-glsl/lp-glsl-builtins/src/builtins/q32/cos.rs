//! Fixed-point 16.16 cosine function.

use super::sin::__lp_q32_sin;

/// Fixed-point value of π (Q16.16 format)
const FIX16_PI: i32 = 205887;

/// Compute cosine using sine: cos(x) = sin(x + π/2)
///
/// Algorithm ported from libfixmath.
/// Accuracy: ~2.1% (same as sin)
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_cos(x: i32) -> i32 {
    // cos(x) = sin(x + π/2)
    let half_pi = FIX16_PI >> 1;
    __lp_q32_sin(x + half_pi)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::test_q32_function_relative;

    #[test]
    fn test_cos_basic() {
        let tests = [
            (0.0, 1.0),
            (1.5707963267948966, 0.0),  // π/2
            (3.141592653589793, -1.0),  // π
            (-1.5707963267948966, 0.0), // -π/2
        ];

        // Use 3% tolerance for trig functions (~2.1% accuracy)
        test_q32_function_relative(|x| __lp_q32_cos(x), &tests, 0.03, 0.01);
    }

    #[test]
    fn test_cos_range_reduction() {
        let tests = [
            (6.283185307179586, 1.0),  // 2π
            (9.42477796076938, -1.0),  // 3π
            (-6.283185307179586, 1.0), // -2π
        ];

        test_q32_function_relative(|x| __lp_q32_cos(x), &tests, 0.03, 0.01);
    }

    #[test]
    fn test_cos_small_angles() {
        let tests = [
            (0.1, 0.9950041652780258),
            (0.5, 0.8775825618903728),
            (-0.1, 0.9950041652780258),
        ];

        test_q32_function_relative(|x| __lp_q32_cos(x), &tests, 0.03, 0.01);
    }
}
