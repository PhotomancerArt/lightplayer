//! Fast fixed-point 16.16 sine function.

/// Fixed-point value of π (Q16.16 format)
const FIX16_PI: i32 = 205887;
const FIX16_TWO_PI: i32 = FIX16_PI << 1;
const FAST_B: i32 = 83_443; // 4 / pi
const FAST_C: i32 = -26_561; // -4 / pi^2
const FAST_P: i32 = 14_746; // 0.225

/// Compute sine using a shader-quality parabolic approximation.
///
/// This is the normal fast rendering path. Reference/debug math should live in
/// probe-specific code rather than making every shader pay for the old Taylor
/// helper path.
#[unsafe(no_mangle)]
pub extern "C" fn __lps_sin_q32(x: i32) -> i32 {
    fast_sin_folded_q32(fold_angle_q32(x))
}

#[inline(always)]
pub(crate) fn fast_sin_q32(x: i32) -> i32 {
    fast_sin_folded_q32(fold_angle_q32(x))
}

#[inline(always)]
pub(crate) fn fold_angle_q32(mut x: i32) -> i32 {
    if x == 0 {
        return 0;
    }
    x %= FIX16_TWO_PI;
    if x > FIX16_PI {
        x -= FIX16_TWO_PI;
    } else if x < -FIX16_PI {
        x += FIX16_TWO_PI;
    }
    x
}

#[inline(always)]
fn fast_sin_folded_q32(x: i32) -> i32 {
    let ax = x.wrapping_abs();
    let y = qmul_wrap(FAST_B, x).wrapping_add(qmul_wrap(FAST_C, qmul_wrap(x, ax)));
    let ay = y.wrapping_abs();
    qmul_trunc_zero(FAST_P, qmul_wrap(y, ay).wrapping_sub(y)).wrapping_add(y)
}

#[inline(always)]
fn qmul_wrap(lhs: i32, rhs: i32) -> i32 {
    (((lhs as i64) * (rhs as i64)) >> 16) as i32
}

#[inline(always)]
fn qmul_trunc_zero(lhs: i32, rhs: i32) -> i32 {
    let product = lhs as i64 * rhs as i64;
    ((product + ((product >> 63) & 0xffff)) >> 16) as i32
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::test_q32_function_relative;

    #[test]
    fn test_sin_basic() {
        let tests = [
            (0.0, 0.0),
            (1.5707963267948966, 1.0),   // π/2
            (3.141592653589793, 0.0),    // π
            (-1.5707963267948966, -1.0), // -π/2
        ];

        // The fast rendering approximation is intentionally shader-quality,
        // not a reference libm replacement.
        test_q32_function_relative(|x| __lps_sin_q32(x), &tests, 0.03, 0.01);
    }

    #[test]
    fn test_sin_range_reduction() {
        let tests = [
            (6.283185307179586, 0.0),  // 2π
            (9.42477796076938, 0.0),   // 3π
            (-6.283185307179586, 0.0), // -2π
        ];

        test_q32_function_relative(|x| __lps_sin_q32(x), &tests, 0.03, 0.01);
    }

    #[test]
    fn test_sin_small_angles() {
        let tests = [
            (1.0 / 65536.0, 1.0 / 65536.0),
            (0.1, 0.09983341664682815),
            (0.5, 0.479425538604203),
            (-0.1, -0.09983341664682815),
        ];

        test_q32_function_relative(|x| __lps_sin_q32(x), &tests, 0.03, 0.01);
    }

    #[test]
    fn test_sin_fast_error_envelope_on_shader_angles() {
        let angles = [
            -4.0 * core::f32::consts::PI,
            -core::f32::consts::PI,
            -core::f32::consts::FRAC_PI_2,
            -0.5,
            0.0,
            0.5,
            core::f32::consts::FRAC_PI_2,
            core::f32::consts::PI,
            4.0 * core::f32::consts::PI,
        ];
        for angle in angles {
            let fixed = crate::util::test_helpers::float_to_fixed(angle);
            let got = crate::util::test_helpers::fixed_to_float(__lps_sin_q32(fixed));
            let expected = angle.sin();
            assert!(
                (got - expected).abs() < 0.012,
                "sin({angle}) got {got}, expected {expected}"
            );
        }
    }
}
