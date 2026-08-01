//! 1D value noise (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/gnoise/gnoise1.glsl` (normative): random values at
//! integer lattice points, cubic-smoothstep interpolation. Value/gradient
//! lattice noise is a standard algorithm from graphics literature (see
//! docs/reports/2026-03-31-lpfx-license-audit.md).
//!
//! **Follows the canonical, not the Q32 implementation:** the Q32 device code
//! approximates the cubic smoothstep with a 256-entry LUT to avoid two
//! fixed-point multiplies; f32 uses the exact polynomial `3t^2 - 2t^3` the
//! canonical specifies. That is a deliberate difference, and it is why this
//! function does not agree pointwise with its Q32 sibling even ignoring the
//! sin-hash.
//!
//! **Tolerance:** built on the chaotic sin-hash `lpfn_random`, so conformance
//! against Q32 is statistical, not pointwise. Exact against the canonical f32.

use crate::builtins::lpfn::generative::random::random1_f32::__lp_lpfn_random1_f32;
use crate::f32_math::{floor, mix};

/// 1D Gradient Noise function (float version).
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_gnoise(float x, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_gnoise1_f32(x: f32, seed: u32) -> f32 {
    let i = floor(x);
    let f = x - i;

    let a = __lp_lpfn_random1_f32(i, seed);
    let b = __lp_lpfn_random1_f32(i + 1.0, seed);

    let u = f * f * (3.0 - 2.0 * f);
    mix(a, b, u)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lattice_points_equal_the_underlying_random() {
        for i in -8..=8 {
            let x = i as f32;
            assert_eq!(__lp_lpfn_gnoise1_f32(x, 0), __lp_lpfn_random1_f32(x, 0));
        }
    }

    #[test]
    fn stays_inside_the_unit_interval() {
        for i in -400..=400 {
            let v = __lp_lpfn_gnoise1_f32(i as f32 * 0.13, 3);
            assert!((0.0..=1.0).contains(&v), "gnoise1 = {v}");
        }
    }

    #[test]
    fn is_continuous_across_a_lattice_boundary() {
        let eps = 1e-4f32;
        let below = __lp_lpfn_gnoise1_f32(1.0 - eps, 0);
        let at = __lp_lpfn_gnoise1_f32(1.0, 0);
        let above = __lp_lpfn_gnoise1_f32(1.0 + eps, 0);
        assert!((below - at).abs() < 1e-2, "{below} vs {at}");
        assert!((above - at).abs() < 1e-2, "{above} vs {at}");
    }
}
