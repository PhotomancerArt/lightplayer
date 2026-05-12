//! Combined fixed-point sine and cosine (shared fast sine approximation).

use crate::builtins::glsl::sin_q32::fast_sin_q32;

/// Fixed-point value of π (Q16.16 format)
const FIX16_PI: i32 = 205887;

/// `sincos(x)` in Q16.16 (`sin` then `cos`). Used from Rust call sites (inlined).
#[inline(always)]
pub fn lps_sincos_q32_pair(x: i32) -> (i32, i32) {
    let half_pi = FIX16_PI >> 1;
    let sin_result = fast_sin_q32(x);
    let cos_result = fast_sin_q32(x.wrapping_add(half_pi));
    (sin_result, cos_result)
}

/// C ABI: writes `(sin, cos)` to out-pointers (matches `__lps_sin_q32` / `__lps_cos_q32` numerically).
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes to caller-provided out-pointers"
)]
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
