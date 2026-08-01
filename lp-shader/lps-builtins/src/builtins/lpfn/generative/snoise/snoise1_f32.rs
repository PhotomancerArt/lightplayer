//! 1D simplex-style gradient noise (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/snoise/snoise1.glsl` (normative).
//!
//! LightPlayer's snoise family is a structural rewrite of simplex noise:
//! gradient selection uses the integer `lpfn_hash` (noiz lineage, MIT) instead
//! of the mod-289 float permute of the stegu/LYGIA original. Algorithm
//! lineage: Stefan Gustavson & Ian McEwan's simplex noise (MIT,
//! <https://github.com/stegu/webgl-noise>) via the noise-rs library.
//! See docs/reports/2026-03-31-lpfx-license-audit.md.
//!
//! **Tolerance:** unlike the `random`/`gnoise` families this one is **not**
//! chaotic — the gradient comes from an exact integer hash, so the only
//! floating-point work is the polynomial falloff. Conformance against the
//! canonical f32 is exact.

use crate::builtins::lpfn::hash::lpfn_hash;
use crate::f32_math::floor;

/// 1D Simplex Noise function (float version). Returns roughly `[-1, 1]`.
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_snoise(float x, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_snoise1_f32(x: f32, seed: u32) -> f32 {
    let cell = floor(x) as i32;
    let dist = x - cell as f32;

    // Hash cell coordinate to pick gradient (+1 or -1).
    let h = lpfn_hash(cell as u32, seed);
    let gradient = if h & 1 == 0 { 1.0 } else { -1.0 };

    let dotv = gradient * dist;

    // Quadratic support: t = 1 - dist^2, quintic falloff inside.
    let t = 1.0 - dist * dist;
    if t > 0.0 {
        let t2 = t * t;
        let t3 = t2 * t;
        let t4 = t2 * t2;
        let t5 = t3 * t2;
        let falloff = 6.0 * t5 - 15.0 * t4 + 10.0 * t3;
        dotv * falloff
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_at_the_lattice_points() {
        // dist == 0 there, so the gradient dot product vanishes.
        for i in -8..=8 {
            assert_eq!(__lp_lpfn_snoise1_f32(i as f32, 0), 0.0);
        }
    }

    #[test]
    fn bounded_and_deterministic() {
        for i in -400..=400 {
            let x = i as f32 * 0.11;
            let v = __lp_lpfn_snoise1_f32(x, 9);
            assert!((-1.0..=1.0).contains(&v), "snoise1({x}) = {v}");
            assert_eq!(v, __lp_lpfn_snoise1_f32(x, 9));
        }
    }

    #[test]
    fn large_coordinates_do_not_wrap() {
        // The Q32 round-trip stub wrapped past ±32768; this must not.
        for x in [1.0e5f32, 1.0e6] {
            let v = __lp_lpfn_snoise1_f32(x + 0.5, 0);
            assert!((-1.0..=1.0).contains(&v), "snoise1({x}) = {v}");
        }
    }
}
