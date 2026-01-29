//! Fixed-point 16.16 arctangent2 function.

use crate::builtins::q32::div::__lp_q32_div;
use crate::builtins::q32::mul::__lp_q32_mul;

/// Fixed-point value of π/4 (Q16.16 format)
const PI_DIV_4: i32 = 0x0000C90F; // 51471

/// Fixed-point value of 3π/4 (Q16.16 format)
const THREE_PI_DIV_4: i32 = 0x00025B2F; // 154415

/// Compute atan2(y, x) using polynomial approximation.
///
/// Algorithm ported from libfixmath.
/// Returns angle in radians in range [-π, π].
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_atan2(y: i32, x: i32) -> i32 {
    // Compute absolute value of y
    let mask = y >> 31;
    let abs_y = (y + mask) ^ mask;

    let base_angle = if x >= 0 {
        // First quadrant: x >= 0
        let r = __lp_q32_div(x - abs_y, x + abs_y);
        let r_3 = __lp_q32_mul(__lp_q32_mul(r, r), r);
        // Polynomial: 0x00003240 * r³ - 0x0000FB50 * r + π/4
        __lp_q32_mul(0x00003240, r_3) - __lp_q32_mul(0x0000FB50, r) + PI_DIV_4
    } else {
        // Second/third quadrant: x < 0
        let r = __lp_q32_div(x + abs_y, abs_y - x);
        let r_3 = __lp_q32_mul(__lp_q32_mul(r, r), r);
        // Polynomial: 0x00003240 * r³ - 0x0000FB50 * r + 3π/4
        __lp_q32_mul(0x00003240, r_3) - __lp_q32_mul(0x0000FB50, r) + THREE_PI_DIV_4
    };

    // Negate if y < 0
    if y < 0 { -base_angle } else { base_angle }
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    #[test]
    fn test_atan2_basic() {
        let tests = [
            ((1.0f32, 1.0f32), 0.7853981633974483f32), // atan2(1, 1) = π/4
            ((1.0f32, 0.0f32), 1.5707963267948966f32), // atan2(1, 0) = π/2
            ((0.0f32, 1.0f32), 0.0f32),                // atan2(0, 1) = 0
            ((-1.0f32, 1.0f32), -0.7853981633974483f32), // atan2(-1, 1) = -π/4
        ];

        for ((y, x), expected) in tests {
            let y_fixed = (y * 65536.0f32).round() as i32;
            let x_fixed = (x * 65536.0f32).round() as i32;
            let result_fixed = __lp_q32_atan2(y_fixed, x_fixed);
            let result = result_fixed as f32 / 65536.0f32;

            std::println!(
                "Test: atan2({}, {}) -> Expected: {}, Actual: {}",
                y,
                x,
                expected,
                result
            );

            assert!(
                (result - expected).abs() < 0.01,
                "Test failed: atan2({}, {}); actual: {}; expected {}",
                y,
                x,
                result,
                expected
            );
        }
    }
}
