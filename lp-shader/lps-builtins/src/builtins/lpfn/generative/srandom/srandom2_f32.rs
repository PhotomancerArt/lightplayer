//! 2D signed random in `[-1, 1]` (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/srandom/srandom2.glsl` (normative): a trivial
//! transform of `lpfn_random`, `-1 + 2 * random(p, seed)` — basic arithmetic
//! applied to our MIT-licensed random
//! (see docs/reports/2026-03-31-lpfx-license-audit.md).
//!
//! **Tolerance:** inherits `lpfn_random`'s chaotic sin-hash, so conformance is
//! statistical, not pointwise. See `random1_f32`.

/// 2D signed random function (float version).
///
/// # Returns
/// Random value in [-1, 1) range as f32
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_srandom(vec2 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_srandom2_f32(x: f32, y: f32, seed: u32) -> f32 {
    -1.0 + 2.0
        * crate::builtins::lpfn::generative::random::random2_f32::__lp_lpfn_random2_f32(x, y, seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_is_signed_and_bounded() {
        for i in -200..=200 {
            let t = i as f32 * 0.23;
            let v = __lp_lpfn_srandom2_f32(t, t, 0);
            assert!((-1.0..1.0).contains(&v), "srandom2({t}) = {v}");
        }
    }

    #[test]
    fn it_is_exactly_the_unsigned_form_remapped() {
        let t = 1.75f32;
        let u =
            crate::builtins::lpfn::generative::random::random2_f32::__lp_lpfn_random2_f32(t, t, 0);
        assert_eq!(__lp_lpfn_srandom2_f32(t, t, 0), -1.0 + 2.0 * u);
    }
}
