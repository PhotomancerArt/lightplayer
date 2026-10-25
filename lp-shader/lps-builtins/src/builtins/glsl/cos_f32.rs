//! Native f32 `cos` — the f32 sibling of [`super::cos_q32`].
//!
//! **Implementation:** [`crate::f32_math::sincos`], single precision
//! throughout, so the LX7's FPU runs it. `libm::cosf` (what this was, and what
//! the `interp.f32` conformance oracle still reaches through
//! `StdMathHandler`) computes its kernel in `f64`, which on the ESP32-S3 is
//! software: the fps spike measured Float mode 4.9× slower than Q32 for it.
//! The two agree within the band below, not bit for bit.
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
//! **Domain:** All finite inputs.

/// GLSL `cos(x)`.
#[unsafe(no_mangle)]
pub extern "C" fn __lps_cos_f32(x: f32) -> f32 {
    crate::f32_math::sincos(x).1
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::util::test_helpers::assert_f32_close;

    #[test]
    fn known_values() {
        assert_f32_close(
            __lps_cos_f32(0.0),
            1.0,
            1e-6,
            1e-6,
            format_args!("cos(0.0)"),
        );
        assert_f32_close(
            __lps_cos_f32(core::f32::consts::PI),
            -1.0,
            1e-6,
            1e-6,
            format_args!("cos(core::f32::consts::PI)"),
        );
    }

    /// The declared band, checked against the f64 reference across the range
    /// shaders actually use.
    #[test]
    fn within_the_declared_band_over_the_sample_range() {
        for i in -40..=40 {
            let x = i as f32 * 0.1;
            let got = __lps_cos_f32(x);
            let want = (x as f64).cos() as f32;
            if !want.is_finite() {
                // float.md §5: out-of-domain results are Unspecified — never asserted.
                continue;
            }
            assert_f32_close(got, want, 1e-6, 1e-6, format_args!("cos({x})"));
        }
    }
}
