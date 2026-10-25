//! Combined native f32 sine and cosine — the f32 sibling of
//! [`super::sincos_q32`].
//!
//! Both come out of one [`crate::f32_math::sincos`] call: one range
//! reduction, two polynomials. `sin_f32` and `cos_f32` are that call's halves.
//!
//! **The results are identical to calling [`super::sin_f32`] and
//! [`super::cos_f32`] separately**, which is the contract the Q32 sibling also
//! promises and the property worth protecting: a shader must not get a
//! different answer depending on which spelling the frontend chose.
//!
//! **Tolerance:** whatever `sin_f32` and `cos_f32` declare, unchanged.

/// `(sin, cos)` for Rust call sites.
#[inline(always)]
pub fn lps_sincos_f32_pair(x: f32) -> (f32, f32) {
    crate::f32_math::sincos(x)
}

/// C ABI: writes `(sin, cos)` to out-pointers.
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes to caller-provided out-pointers"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lps_sincos_f32(x: f32, sin_out: *mut f32, cos_out: *mut f32) {
    let (s, c) = lps_sincos_f32_pair(x);
    unsafe {
        *sin_out = s;
        *cos_out = c;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins::glsl::{cos_f32::__lps_cos_f32, sin_f32::__lps_sin_f32};

    #[test]
    fn agrees_bit_for_bit_with_the_separate_builtins() {
        for i in -60..=60 {
            let x = i as f32 * 0.1;
            let (s, c) = lps_sincos_f32_pair(x);
            assert_eq!(s.to_bits(), __lps_sin_f32(x).to_bits(), "sin({x})");
            assert_eq!(c.to_bits(), __lps_cos_f32(x).to_bits(), "cos({x})");
        }
    }

    #[test]
    fn writes_both_out_pointers() {
        let (mut s, mut c) = (0.0f32, 0.0f32);
        __lps_sincos_f32(0.0, &mut s, &mut c);
        assert_eq!(s, 0.0);
        assert_eq!(c, 1.0);
    }

    #[test]
    fn pythagorean_identity_holds() {
        for i in -60..=60 {
            let x = i as f32 * 0.1;
            let (s, c) = lps_sincos_f32_pair(x);
            assert!((s * s + c * c - 1.0).abs() < 1e-6, "at {x}");
        }
    }
}
