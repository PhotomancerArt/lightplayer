//! Fixed-point 16.16 tangent function.

use super::cos::__lp_q32_cos;
use super::sin::__lp_q32_sin;
use crate::builtins::q32::div::__lp_q32_div;

/// Compute tangent using sine and cosine: tan(x) = sin(x) / cos(x)
///
/// Algorithm ported from libfixmath.
/// Accuracy: ~2.1% (same as sin/cos)
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_tan(x: i32) -> i32 {
    let sin_val = __lp_q32_sin(x);
    let cos_val = __lp_q32_cos(x);
    __lp_q32_div(sin_val, cos_val)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::test_q32_function_relative;

    #[test]
    fn test_tan_basic() {
        let tests = [
            (0.0, 0.0),
            (0.7853981633974483, 1.0),   // π/4
            (-0.7853981633974483, -1.0), // -π/4
        ];

        // Use 3% tolerance for trig functions (~2.1% accuracy)
        test_q32_function_relative(|x| __lp_q32_tan(x), &tests, 0.03, 0.01);
    }

    #[test]
    fn test_tan_small_angles() {
        let tests = [
            (0.1, 0.10033467208545055),
            (0.5, 0.5463024898437905),
            (-0.1, -0.10033467208545055),
        ];

        test_q32_function_relative(|x| __lp_q32_tan(x), &tests, 0.03, 0.01);
    }

    #[test]
    fn test_tan_range_reduction() {
        let tests = [
            (3.141592653589793, 0.0), // π (should be ~0)
            (6.283185307179586, 0.0), // 2π (should be ~0)
        ];

        // Use larger tolerance for values near zero
        test_q32_function_relative(|x| __lp_q32_tan(x), &tests, 0.05, 0.01);
    }
}
