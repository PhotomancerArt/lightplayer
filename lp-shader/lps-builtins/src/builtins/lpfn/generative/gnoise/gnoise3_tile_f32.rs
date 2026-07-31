//! Tilable 3D gradient noise (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/gnoise/gnoise3_tile.glsl` (normative): gradients from
//! `lpfn_srandom3_tile` at the eight cell corners, dotted with corner offsets,
//! trilinear interpolation with quintic weights, normalized to `[0, 1]`.
//! `tileLength == 0` falls back to the non-tiling `lpfn_gnoise(vec3)` remapped
//! to `[0, 1]`. Standard tilable lattice gradient noise (see
//! docs/reports/2026-03-31-lpfx-license-audit.md).
//!
//! **Tolerance:** built on the chaotic sin-hash `lpfn_srandom3_vec` —
//! statistical vs Q32, exact vs the canonical f32.

use super::gnoise3_f32::{__lp_lpfn_gnoise3_f32, quintic};
use crate::builtins::lpfn::generative::srandom::srandom3_tile_f32::srandom3_tile;
use crate::f32_math::{floor, fract, mix};

#[inline(always)]
fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Tilable 3D Gradient Noise function (float version). Returns `[0, 1]`.
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_gnoise(vec3 p, float tileLength, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_gnoise3_tile_f32(
    x: f32,
    y: f32,
    z: f32,
    tile_length: f32,
    seed: u32,
) -> f32 {
    if tile_length == 0.0 {
        // Normalize gnoise3 output from [-1, 1] to [0, 1].
        return __lp_lpfn_gnoise3_f32(x, y, z, seed) * 0.5 + 0.5;
    }

    let i = [floor(x), floor(y), floor(z)];
    let f = [fract(x), fract(y), fract(z)];
    let u = [quintic(f[0]), quintic(f[1]), quintic(f[2])];

    // Matches the canonical: tileLength * lacunarity(2.0) * 0.5 == tileLength.
    let scaled_tile = tile_length;

    // The eight corners, in the canonical's order.
    const CORNERS: [[f32; 3]; 8] = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [0.0, 1.0, 1.0],
        [1.0, 1.0, 1.0],
    ];

    let mut d = [0.0f32; 8];
    for (k, c) in CORNERS.iter().enumerate() {
        let offset = [f[0] - c[0], f[1] - c[1], f[2] - c[2]];
        let g = srandom3_tile(i[0] + c[0], i[1] + c[1], i[2] + c[2], scaled_tile);
        d[k] = dot3(g, offset);
    }

    let x00 = mix(d[0], d[1], u[0]);
    let x10 = mix(d[2], d[3], u[0]);
    let x01 = mix(d[4], d[5], u[0]);
    let x11 = mix(d[6], d[7], u[0]);

    let y0 = mix(x00, x10, u[1]);
    let y1 = mix(x01, x11, u[1]);

    let result = mix(y0, y1, u[2]);

    // Normalize to [0, 1].
    result * 0.5 + 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_tile_length_falls_back_to_the_non_tiling_form() {
        for i in -6..=6 {
            let t = i as f32 * 0.4;
            assert_eq!(
                __lp_lpfn_gnoise3_tile_f32(t, t * 1.3, t * 0.7, 0.0, 2),
                __lp_lpfn_gnoise3_f32(t, t * 1.3, t * 0.7, 2) * 0.5 + 0.5
            );
        }
    }

    #[test]
    fn it_repeats_at_the_tile_period() {
        let period = 4.0f32;
        for i in 0..6 {
            let p = i as f32 * 0.5;
            let a = __lp_lpfn_gnoise3_tile_f32(p, p, p, period, 0);
            let b = __lp_lpfn_gnoise3_tile_f32(p + period, p + period, p + period, period, 0);
            assert!((a - b).abs() < 1e-5, "at {p}: {a} vs {b}");
        }
    }
}
