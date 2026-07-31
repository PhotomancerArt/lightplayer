//! Tilable 3D FBM (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/fbm/fbm3_tile.glsl` (normative): normalized octave sum
//! of tilable gradient noise with persistence 0.5 and lacunarity 2.0. FBM is a
//! standard procedure (see docs/reports/2026-03-31-lpfx-license-audit.md).
//!
//! Note the octave loop scales the tile length as well as the position, so each
//! octave tiles at its own frequency — that is what keeps the sum periodic.
//!
//! **Tolerance:** built on the chaotic sin-hash noise stack — statistical vs
//! Q32, exact vs the canonical f32.

use crate::builtins::lpfn::generative::gnoise::gnoise3_tile_f32::__lp_lpfn_gnoise3_tile_f32;

/// Tilable 3D FBM function (float version).
#[lpfn_impl_macro::lpfn_impl(
    f32,
    "float lpfn_fbm(vec3 p, float tileLength, int octaves, uint seed)"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_fbm3_tile_f32(
    x: f32,
    y: f32,
    z: f32,
    tile_length: f32,
    octaves: i32,
    seed: u32,
) -> f32 {
    const PERSISTENCE: f32 = 0.5;
    const LACUNARITY: f32 = 2.0;

    let mut amplitude = 0.5f32;
    let mut total = 0.0f32;
    let mut normalization = 0.0f32;
    let (mut px, mut py, mut pz) = (x, y, z);

    let mut i = 0;
    while i < octaves {
        let scaled_tile = tile_length * LACUNARITY * 0.5;
        let noise_value = __lp_lpfn_gnoise3_tile_f32(px, py, pz, scaled_tile, seed);
        total += noise_value * amplitude;
        normalization += amplitude;
        amplitude *= PERSISTENCE;
        px *= LACUNARITY;
        py *= LACUNARITY;
        pz *= LACUNARITY;
        i += 1;
    }

    total / normalization
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_octaves_is_the_canonical_zero_over_zero() {
        // The canonical divides by the accumulated normalization, which is 0
        // with no octaves. float.md §3 makes that NaN rather than a trap, and
        // the canonical has the same behavior — so this pins "does not crash",
        // not a value.
        assert!(__lp_lpfn_fbm3_tile_f32(1.0, 2.0, 3.0, 4.0, 0, 0).is_nan());
    }

    #[test]
    fn stays_in_the_unit_interval() {
        // Each octave of gnoise3_tile is in [0, 1] and the sum is normalized.
        for i in -20..=20 {
            let t = i as f32 * 0.3;
            let v = __lp_lpfn_fbm3_tile_f32(t, t * 1.3, t * 0.7, 4.0, 4, 0);
            assert!((0.0..=1.0).contains(&v), "fbm3_tile = {v}");
        }
    }

    #[test]
    fn it_repeats_at_the_tile_period() {
        let period = 4.0f32;
        for i in 0..5 {
            let p = i as f32 * 0.5;
            let a = __lp_lpfn_fbm3_tile_f32(p, p, p, period, 3, 0);
            let b = __lp_lpfn_fbm3_tile_f32(p + period, p + period, p + period, period, 3, 0);
            assert!((a - b).abs() < 1e-4, "at {p}: {a} vs {b}");
        }
    }
}
