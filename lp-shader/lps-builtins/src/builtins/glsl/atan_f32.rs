//! Native f32 `atan` — the f32 sibling of [`super::atan_q32`].
//!
//! **Implementation:** `libm::atanf`. `libm` is a `no_std` workspace
//! dependency and is the same code the `interp.f32` conformance oracle reaches
//! through `StdMathHandler`, so the builtin and the oracle agree by
//! construction rather than by tuning.
//!
//! **Tolerance (`docs/design/float.md` §6):** `1e-6` relative with a `1e-6`
//! absolute floor, against the f64 reference. That is f32 round-off, not a
//! quality allowance — the canonical GLSL definition *is* the IEEE function
//! here, so there is no algorithmic error to budget for.
//!
//! The Q32 sibling is a fixed-point approximation with a much wider band; this
//! one does not inherit that, because in f32 the accurate implementation is
//! also the cheap one. Where an approximation would genuinely be faster —
//! `inversesqrt` is the real case — it is spelled out in that file with its own
//! band. Speed-over-ulp (roadmap D6) is a licence, not an obligation.
//!
//! **Domain:** All finite inputs; `±inf` gives `±pi/2`.

/// GLSL `atan(x)`.
#[unsafe(no_mangle)]
pub extern "C" fn __lps_atan_f32(x: f32) -> f32 {
    libm::atanf(x)
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::util::test_helpers::assert_f32_close;

    #[test]
    fn known_values() {
        assert_f32_close(
            __lps_atan_f32(0.0),
            0.0,
            1e-6,
            1e-6,
            format_args!("atan(0.0)"),
        );
        assert_f32_close(
            __lps_atan_f32(1.0),
            core::f32::consts::FRAC_PI_4,
            1e-6,
            1e-6,
            format_args!("atan(1.0)"),
        );
    }

    /// The declared band, checked against the f64 reference across the range
    /// shaders actually use.
    #[test]
    fn within_the_declared_band_over_the_sample_range() {
        for i in -40..=40 {
            let x = i as f32 * 0.1;
            let got = __lps_atan_f32(x);
            let want = (x as f64).atan() as f32;
            if !want.is_finite() {
                // float.md §5: out-of-domain results are Unspecified — never asserted.
                continue;
            }
            assert_f32_close(got, want, 1e-6, 1e-6, format_args!("atan({x})"));
        }
    }
}
