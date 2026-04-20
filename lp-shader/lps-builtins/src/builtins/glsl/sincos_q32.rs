//! Combined fixed-point sine and cosine (shared range folding; cos via `sin(θ + π/2)`).

use crate::builtins::lpir::fmul_q32::__lp_lpir_fmul_q32;

/// Fixed-point value of π (Q16.16 format)
const FIX16_PI: i32 = 205887;

/// Fold `x` to `[-π, π]` (same logic as `__lps_sin_q32`).
#[inline(always)]
fn fold_angle(mut x: i32) -> i32 {
    if x == 0 {
        return 0;
    }
    let two_pi = FIX16_PI << 1;
    x %= two_pi;
    if x > FIX16_PI {
        x -= two_pi;
    } else if x < -FIX16_PI {
        x += two_pi;
    }
    x
}

/// Taylor series for sin on a pre-folded angle (matches `__lps_sin_q32` body).
#[inline(always)]
fn taylor_sin(temp_angle: i32) -> i32 {
    if temp_angle == 0 {
        return 0;
    }
    let temp_angle_sq = __lp_lpir_fmul_q32(temp_angle, temp_angle);
    let mut result = temp_angle;
    let mut term = __lp_lpir_fmul_q32(temp_angle, temp_angle_sq);
    result -= term / 6;
    term = __lp_lpir_fmul_q32(term, temp_angle_sq);
    result += term / 120;
    term = __lp_lpir_fmul_q32(term, temp_angle_sq);
    result -= term / 5040;
    term = __lp_lpir_fmul_q32(term, temp_angle_sq);
    result += term / 362880;
    term = __lp_lpir_fmul_q32(term, temp_angle_sq);
    result -= term / 39916800;
    result
}

/// `sincos(x)` in Q16.16 (`sin` then `cos`). Used from Rust call sites (inlined).
#[inline(always)]
pub fn lps_sincos_q32_pair(x: i32) -> (i32, i32) {
    let half_pi = FIX16_PI >> 1;
    let sin_result = taylor_sin(fold_angle(x));
    let cos_result = taylor_sin(fold_angle(x.wrapping_add(half_pi)));
    (sin_result, cos_result)
}

/// C ABI: writes `(sin, cos)` to out-pointers (matches `__lps_sin_q32` / `__lps_cos_q32` numerically).
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn __lps_sincos_q32(x: i32, sin_out: *mut i32, cos_out: *mut i32) {
    let (s, c) = lps_sincos_q32_pair(x);
    unsafe {
        *sin_out = s;
        *cos_out = c;
    }
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::builtins::glsl::cos_q32::__lps_cos_q32;
    use crate::builtins::glsl::sin_q32::__lps_sin_q32;
    use crate::util::test_helpers::{fixed_to_float, float_to_fixed};

    #[test]
    fn sincos_matches_sin_cos() {
        for deg in [-180, -90, -45, 0, 30, 45, 60, 90, 123, 180] {
            let rad = (deg as f32) * core::f32::consts::PI / 180.0;
            let x = float_to_fixed(rad);
            let (s, c) = lps_sincos_q32_pair(x);
            let s_exp = __lps_sin_q32(x);
            let c_exp = __lps_cos_q32(x);
            assert_eq!(s, s_exp, "sin mismatch at {deg}°");
            assert_eq!(c, c_exp, "cos mismatch at {deg}°");
        }
    }

    #[test]
    fn sincos_quadrants() {
        let (s0, c0) = lps_sincos_q32_pair(0);
        assert_eq!(s0, 0);
        assert!((c0 - 65536).abs() <= 2);

        let pi2 = float_to_fixed(core::f32::consts::FRAC_PI_2);
        let (s1, c1) = lps_sincos_q32_pair(pi2);
        assert!(fixed_to_float(s1) > 0.99 && fixed_to_float(s1) <= 1.01);
        assert!(fixed_to_float(c1).abs() < 0.05);
    }
}
