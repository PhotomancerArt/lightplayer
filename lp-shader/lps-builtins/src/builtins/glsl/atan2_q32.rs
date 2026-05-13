//! Fixed-point 16.16 arctangent2 function.

use crate::builtins::lpir::fdiv_q32::__lp_lpir_fdiv_q32;
use crate::builtins::lpir::fmul_q32::__lp_lpir_fmul_q32;

/// Fixed-point value of π/4 (Q16.16 format)
const PI_DIV_4: i32 = 0x0000C90F; // 51471

/// Fixed-point value of 3π/4 (Q16.16 format)
const THREE_PI_DIV_4: i32 = 0x00025B2F; // 154415

/// Compute atan2(y, x) using polynomial approximation.
///
/// Algorithm ported from libfixmath.
/// Returns angle in radians in range [-π, π].
#[unsafe(no_mangle)]
pub extern "C" fn __lps_atan2_q32(y: i32, x: i32) -> i32 {
    // GLSL atan(y,x) is undefined at (0,0); our first-quadrant formula uses
    // div(x - |y|, x + |y|) which hits div(0,0) when both are zero. Saturating
    // div returns MAX_FIXED and the polynomial becomes garbage — visible as
    // high-frequency "static" when shaders use atan2 on near-zero gradients.
    if x == 0 && y == 0 {
        return 0;
    }

    // Compute absolute value of y
    let mask = y >> 31;
    let abs_y = sat_add_i32(y, mask) ^ mask;

    let base_angle = if x >= 0 {
        // First quadrant: x >= 0
        let r = __lp_lpir_fdiv_q32(sat_sub_i32(x, abs_y), sat_add_i32(x, abs_y));
        let r_3 = __lp_lpir_fmul_q32(__lp_lpir_fmul_q32(r, r), r);
        // Polynomial: 0x00003240 * r³ - 0x0000FB50 * r + π/4
        sat_add_i32(
            sat_sub_i32(
                __lp_lpir_fmul_q32(0x00003240, r_3),
                __lp_lpir_fmul_q32(0x0000FB50, r),
            ),
            PI_DIV_4,
        )
    } else {
        // Second/third quadrant: x < 0
        let r = __lp_lpir_fdiv_q32(sat_add_i32(x, abs_y), sat_sub_i32(abs_y, x));
        let r_3 = __lp_lpir_fmul_q32(__lp_lpir_fmul_q32(r, r), r);
        // Polynomial: 0x00003240 * r³ - 0x0000FB50 * r + 3π/4
        sat_add_i32(
            sat_sub_i32(
                __lp_lpir_fmul_q32(0x00003240, r_3),
                __lp_lpir_fmul_q32(0x0000FB50, r),
            ),
            THREE_PI_DIV_4,
        )
    };

    // Negate if y < 0
    if y < 0 {
        base_angle.saturating_neg()
    } else {
        base_angle
    }
}

fn sat_add_i32(lhs: i32, rhs: i32) -> i32 {
    let sum = i64::from(lhs) + i64::from(rhs);
    sum.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn sat_sub_i32(lhs: i32, rhs: i32) -> i32 {
    let diff = i64::from(lhs) - i64::from(rhs);
    diff.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    #[test]
    fn test_atan2_origin_returns_zero() {
        assert_eq!(__lps_atan2_q32(0, 0), 0);
    }

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
            let result_fixed = __lps_atan2_q32(y_fixed, x_fixed);
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

    #[test]
    fn test_atan2_saturated_inputs_do_not_panic() {
        let max = i32::MAX;
        let min = i32::MIN;

        let _ = __lps_atan2_q32(max, max);
        let _ = __lps_atan2_q32(max, min);
        let _ = __lps_atan2_q32(min, max);
        let _ = __lps_atan2_q32(min, min);
    }
}
