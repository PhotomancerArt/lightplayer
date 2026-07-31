//! Native f32 `inversesqrt` — the f32 sibling of [`super::inversesqrt_q32`].
//!
//! **This is the one transcendental where approximating is worth it.** Every
//! other f32 builtin in this directory delegates to `libm` because the accurate
//! implementation is also the cheap one; `inversesqrt` is different, because
//! the honest spelling is `1.0 / sqrt(x)` — a square root *and* a division,
//! both multi-cycle on every target we ship — and a single Newton refinement of
//! the classic bit-trick estimate lands well inside a shader-visible tolerance
//! for a fraction of the cost. `inversesqrt` is also the hottest of the family:
//! it is in the middle of every `normalize`.
//!
//! **Tolerance (`docs/design/float.md` §6):** `2e-3` relative. One Newton step
//! from the magic-constant seed converges to about 0.17% worst case; the band
//! is that, rounded out. This is a *declared* deviation from the canonical
//! GLSL, per roadmap D6 — not an accident, and the test below is what holds it
//! to the number.
//!
//! Callers that need the correctly-rounded reciprocal square root should spell
//! `1.0 / sqrt(x)`, which lowers to the §3 Guaranteed `sqrt` and `/`.
//!
//! **Domain:** GLSL leaves `inversesqrt(x)` undefined for `x <= 0`
//! (float.md §5). The Q32 sibling returns `0` there; this returns `inf` for
//! `+0` and NaN for negatives, which is what the arithmetic produces. Neither
//! is asserted anywhere.
//!
//! LICENSE: the reciprocal-square-root seed constant `0x5f37_5a86` and the
//! Newton refinement are standard published numerical technique (Blinn 1997,
//! Lomont 2003), not adapted source.

/// GLSL `inversesqrt(x)` = `1 / sqrt(x)`, to within 2e-3 relative.
#[unsafe(no_mangle)]
pub extern "C" fn __lps_inversesqrt_f32(x: f32) -> f32 {
    // NaN is tested explicitly: `x <= 0.0` is false for it, so a NaN would
    // otherwise reach the bit-trick estimate and come back as a nonsense
    // finite value rather than propagating.
    if x.is_nan() {
        return f32::NAN;
    }
    if x <= 0.0 {
        return if x == 0.0 { f32::INFINITY } else { f32::NAN };
    }
    let half = 0.5 * x;
    // Seed from the exponent-halving bit trick, then one Newton step of
    // f(y) = 1/y^2 - x.
    let mut y = f32::from_bits(0x5f37_5a86 - (x.to_bits() >> 1));
    y *= 1.5 - half * y * y;
    y
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    /// The declared band, over the range shaders actually feed it — vector
    /// lengths, squared distances, and the small values `normalize` produces
    /// near the origin.
    #[test]
    fn within_two_permille_of_the_reference() {
        let mut worst = 0.0f32;
        for i in 1..=20_000u32 {
            let x = i as f32 * 0.01;
            let got = __lps_inversesqrt_f32(x);
            let want = (1.0f64 / (x as f64).sqrt()) as f32;
            let rel = ((got - want) / want).abs();
            worst = worst.max(rel);
            assert!(rel <= 2e-3, "inversesqrt({x}): got {got}, want {want}");
        }
        // Pin the actual figure so a regression that stays inside the band is
        // still visible in a diff.
        assert!(worst < 2e-3, "worst relative error {worst}");
    }

    #[test]
    fn holds_across_a_wide_exponent_range() {
        for e in -30i32..=30 {
            let x = libm::powf(2.0, e as f32);
            let got = __lps_inversesqrt_f32(x);
            let want = (1.0f64 / (x as f64).sqrt()) as f32;
            let rel = ((got - want) / want).abs();
            assert!(rel <= 2e-3, "inversesqrt(2^{e}) = {got}, want {want}");
        }
    }

    #[test]
    fn out_of_domain_does_not_trap_or_produce_garbage() {
        // float.md §5: values are Unspecified, so only the "no garbage bit
        // pattern, no trap" property is checked.
        assert!(__lps_inversesqrt_f32(-1.0).is_nan());
        assert!(__lps_inversesqrt_f32(f32::NAN).is_nan());
        assert_eq!(__lps_inversesqrt_f32(0.0), f32::INFINITY);
    }
}
