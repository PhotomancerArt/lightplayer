//! Native f32 `pow` — the f32 sibling of [`super::pow_q32`].
//!
//! **Implementation:** `libm::powf`.
//!
//! **Tolerance (`docs/design/float.md` §6):** `1e-6` relative with a `1e-6`
//! absolute floor against the f64 reference.
//!
//! **Domain:** GLSL says `pow(x, y)` is undefined for `x < 0`, and for `x = 0`
//! with `y <= 0` — float.md §5 Unspecified. `libm::powf` still answers (real
//! result for integer exponents, NaN otherwise) and never traps, but nothing
//! may depend on which. The corpus must not assert those inputs.

/// GLSL `pow(x, y)`.
#[unsafe(no_mangle)]
pub extern "C" fn __lps_pow_f32(x: f32, y: f32) -> f32 {
    libm::powf(x, y)
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::util::test_helpers::assert_f32_close;

    #[test]
    fn known_values() {
        assert_f32_close(
            __lps_pow_f32(2.0, 10.0),
            1024.0,
            1e-6,
            1e-6,
            format_args!("pow(2,10)"),
        );
        assert_f32_close(
            __lps_pow_f32(9.0, 0.5),
            3.0,
            1e-6,
            1e-6,
            format_args!("pow(9,0.5)"),
        );
        assert_f32_close(
            __lps_pow_f32(2.0, -2.0),
            0.25,
            1e-6,
            1e-6,
            format_args!("pow(2,-2)"),
        );
        assert_f32_close(
            __lps_pow_f32(5.0, 0.0),
            1.0,
            1e-6,
            1e-6,
            format_args!("pow(5,0)"),
        );
    }

    #[test]
    fn range_reaches_far_past_q32() {
        // Q32 saturates near 32768; this is the capability being added.
        assert_f32_close(
            __lps_pow_f32(10.0, 20.0),
            1e20,
            1e-6,
            1e-6,
            format_args!("pow(10,20)"),
        );
    }

    #[test]
    fn within_the_declared_band_over_the_positive_domain() {
        for ix in 1..=20 {
            for iy in -10..=10 {
                let (x, y) = (ix as f32 * 0.5, iy as f32 * 0.4);
                let want = (x as f64).powf(y as f64) as f32;
                if !want.is_finite() {
                    continue;
                }
                assert_f32_close(
                    __lps_pow_f32(x, y),
                    want,
                    1e-6,
                    1e-6,
                    format_args!("pow({x},{y})"),
                );
            }
        }
    }

    #[test]
    fn negative_base_does_not_trap() {
        // float.md §5: the *value* is Unspecified, so only "it returned" is
        // asserted — never what it returned.
        let _ = __lps_pow_f32(-2.0, 0.5);
        let _ = __lps_pow_f32(0.0, -1.0);
    }
}
