//! 3D fractal Brownian motion (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/fbm/fbm3.glsl` (normative): octave sum of
//! `lpfn_snoise` with amplitude 0.5, gain 0.5, lacunarity 2.0. FBM is a
//! standard procedure from Perlin's 1985 paper (see
//! docs/reports/2026-03-31-lpfx-license-audit.md).
//!
//! **Tolerance:** exact against the canonical f32.

use crate::builtins::lpfn::generative::snoise::snoise3_f32::__lp_lpfn_snoise3_f32;

/// 3D FBM function (float version).
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_fbm(vec3 p, int octaves, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_fbm3_f32(x: f32, y: f32, z: f32, octaves: i32, seed: u32) -> f32 {
    let mut value = 0.0f32;
    let mut amplitude = 0.5f32;
    let (mut sx, mut sy, mut sz) = (x, y, z);
    let mut i = 0;
    while i < octaves {
        value += amplitude * __lp_lpfn_snoise3_f32(sx, sy, sz, seed);
        sx *= 2.0;
        sy *= 2.0;
        sz *= 2.0;
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
        assert_eq!(__lp_lpfn_fbm3_f32(1.0, 2.0, 3.0, 0, 0), 0.0);
    }

    #[test]
    fn one_octave_is_half_the_base_noise() {
        assert_eq!(
            __lp_lpfn_fbm3_f32(1.3, 2.7, 0.4, 1, 0),
            0.5 * __lp_lpfn_snoise3_f32(1.3, 2.7, 0.4, 0)
        );
    }

    #[test]
    fn bounded_over_many_octaves() {
        for i in -20..=20 {
            let t = i as f32 * 0.4;
            let v = __lp_lpfn_fbm3_f32(t, t * 1.7, t * 0.3, 8, 1);
            assert!(v.is_finite() && v.abs() <= 2.0, "fbm3 = {v}");
        }
    }
}
