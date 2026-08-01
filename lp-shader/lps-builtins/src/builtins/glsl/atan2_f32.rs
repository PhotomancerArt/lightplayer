//! Native f32 two-argument `atan` — the f32 sibling of [`super::atan2_q32`].
//!
//! **Implementation:** `libm::atan2f`, which carries the quadrant and
//! signed-zero rules IEEE specifies and a hand-rolled `atan(y/x)` does not:
//! `atan2(1, -1)` is `3pi/4`, not `-pi/4`, and `atan2(0, -1)` is `pi` rather
//! than `0`. Those are the cases every naive implementation gets wrong.
//!
//! **Tolerance (`docs/design/float.md` §6):** `1e-6` relative with a `1e-6`
//! absolute floor against the f64 reference.
//!
//! **Domain:** all finite inputs, including `x = 0` (gives `±pi/2`).
//! `atan2(0, 0)` is GLSL-undefined (float.md §5) and returns `0` here without
//! trapping; do not depend on it.

/// GLSL `atan(y, x)`.
#[unsafe(no_mangle)]
pub extern "C" fn __lps_atan2_f32(y: f32, x: f32) -> f32 {
    libm::atan2f(y, x)
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::util::test_helpers::assert_f32_close;
    use core::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

    #[test]
    fn quadrants_are_right() {
        // The whole reason this is not `atan(y / x)`.
        assert_f32_close(
            __lps_atan2_f32(1.0, 1.0),
            FRAC_PI_4,
            1e-6,
            1e-6,
            format_args!("atan2(1,1)"),
        );
        assert_f32_close(
            __lps_atan2_f32(1.0, -1.0),
            3.0 * FRAC_PI_4,
            1e-6,
            1e-6,
            format_args!("atan2(1,-1)"),
        );
        assert_f32_close(
            __lps_atan2_f32(-1.0, -1.0),
            -3.0 * FRAC_PI_4,
            1e-6,
            1e-6,
            format_args!("atan2(-1,-1)"),
        );
        assert_f32_close(
            __lps_atan2_f32(0.0, -1.0),
            PI,
            1e-6,
            1e-6,
            format_args!("atan2(0,-1)"),
        );
        assert_f32_close(
            __lps_atan2_f32(1.0, 0.0),
            FRAC_PI_2,
            1e-6,
            1e-6,
            format_args!("atan2(1,0)"),
        );
    }

    #[test]
    fn within_the_declared_band_over_the_sample_range() {
        for iy in -8..=8 {
            for ix in -8..=8 {
                let (y, x) = (iy as f32 * 0.5, ix as f32 * 0.5);
                if y == 0.0 && x == 0.0 {
                    // float.md §5: Unspecified — never asserted.
                    continue;
                }
                let want = (y as f64).atan2(x as f64) as f32;
                assert_f32_close(
                    __lps_atan2_f32(y, x),
                    want,
                    1e-6,
                    1e-6,
                    format_args!("atan2({y},{x})"),
                );
            }
        }
    }
}
