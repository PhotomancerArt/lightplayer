//! 2D fractal Brownian motion (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/fbm/fbm2.glsl` (normative): octave sum of
//! `lpfn_snoise` with amplitude 0.5, gain 0.5, lacunarity 2.0. FBM (weighted
//! octave sum) is a standard procedure from Perlin's 1985 paper (see
//! docs/reports/2026-03-31-lpfx-license-audit.md).
//!
//! **Tolerance:** exact against the canonical f32; inherits `snoise2`'s
//! exactness.
//!
//! The octave loop is where f32 range earns its keep: each octave doubles the
//! coordinate, so eight octaves of a coordinate near 100 reaches 25 600 —
//! comfortably inside f32 and uncomfortably close to Q16.16's ±32768 ceiling.

use crate::builtins::lpfn::generative::snoise::snoise2_f32::__lp_lpfn_snoise2_f32;

/// 2D FBM function (float version).
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_fbm(vec2 p, int octaves, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_fbm2_f32(x: f32, y: f32, octaves: i32, seed: u32) -> f32 {
    let mut value = 0.0f32;
    let mut amplitude = 0.5f32;
    let (mut sx, mut sy) = (x, y);
    let mut i = 0;
    while i < octaves {
        value += amplitude * __lp_lpfn_snoise2_f32(sx, sy, seed);
        sx *= 2.0;
        sy *= 2.0;
        amplitude *= 0.5;
        i += 1;
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_octaves_is_zero() {
        assert_eq!(__lp_lpfn_fbm2_f32(1.0, 2.0, 0, 0), 0.0);
    }

    #[test]
    fn one_octave_is_half_the_base_noise() {
        let x = 1.3;
        let y = 2.7;
        assert_eq!(
            __lp_lpfn_fbm2_f32(x, y, 1, 0),
            0.5 * __lp_lpfn_snoise2_f32(x, y, 0)
        );
    }

    #[test]
    fn negative_octave_counts_do_not_loop_forever() {
        assert_eq!(__lp_lpfn_fbm2_f32(1.0, 2.0, -3, 0), 0.0);
    }

    #[test]
    fn bounded_over_many_octaves() {
        for i in -20..=20 {
            let v = __lp_lpfn_fbm2_f32(i as f32 * 0.7, i as f32 * 0.3, 8, 1);
            assert!(v.is_finite() && v.abs() <= 2.0, "fbm2 = {v}");
        }
    }
}
