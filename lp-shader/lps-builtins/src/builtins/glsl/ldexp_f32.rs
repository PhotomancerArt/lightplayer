//! Native f32 `ldexp` — the f32 sibling of [`super::ldexp_q32`].
//!
//! `ldexp(x, e) = x * 2^e`. The Q32 sibling implements it as a raw shift of the
//! fixed-point word, which is exact but silently wraps on overflow. In f32 the
//! operation is an exponent add, so `libm::ldexpf` does it exactly and
//! overflows to `±inf` / underflows toward zero per IEEE instead of wrapping.
//!
//! **Tolerance:** exact when the result is representable — this scales by a
//! power of two, so no rounding occurs until the result leaves the normal
//! range.

/// GLSL `ldexp(x, exp)` = `x * 2^exp`.
#[unsafe(no_mangle)]
pub extern "C" fn __lps_ldexp_f32(x: f32, exp: i32) -> f32 {
    libm::ldexpf(x, exp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_by_powers_of_two_exactly() {
        assert_eq!(__lps_ldexp_f32(1.0, 0), 1.0);
        assert_eq!(__lps_ldexp_f32(1.0, 3), 8.0);
        assert_eq!(__lps_ldexp_f32(1.0, -3), 0.125);
        assert_eq!(__lps_ldexp_f32(-3.5, 2), -14.0);
        assert_eq!(__lps_ldexp_f32(0.0, 10), 0.0);
    }

    #[test]
    fn overflows_to_infinity_rather_than_wrapping() {
        // The Q32 sibling shifts the raw word and wraps; f32 must not.
        assert_eq!(__lps_ldexp_f32(1.0, 200), f32::INFINITY);
        assert_eq!(__lps_ldexp_f32(-1.0, 200), f32::NEG_INFINITY);
        assert_eq!(__lps_ldexp_f32(1.0, -200), 0.0);
    }

    #[test]
    fn agrees_with_multiplication_where_both_are_in_range() {
        for e in -20i32..=20 {
            for m in [1.0f32, 1.5, -2.25, 7.0] {
                assert_eq!(__lps_ldexp_f32(m, e), m * libm::powf(2.0, e as f32));
            }
        }
    }
}
