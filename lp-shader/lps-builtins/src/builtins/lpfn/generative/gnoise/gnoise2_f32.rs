//! 2D value noise (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/gnoise/gnoise2.glsl` (normative): random values at the
//! four cell corners, bilinear interpolation with cubic-smoothstep weights.
//! Standard lattice value noise (see
//! docs/reports/2026-03-31-lpfx-license-audit.md; originally written with
//! reference to LYGIA's gnoise.glsl).
//!
//! **Follows the canonical, not the Q32 implementation:** the Q32 device code
//! uses a 256-entry smoothstep LUT; this uses the exact `3t^2 - 2t^3`.
//!
//! **Tolerance:** chaotic sin-hash underneath — statistical vs Q32, exact vs
//! the canonical f32.

use crate::builtins::lpfn::generative::random::random2_f32::__lp_lpfn_random2_f32;
use crate::f32_math::{floor, fract, mix};

/// 2D Gradient Noise function (float version).
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_gnoise(vec2 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_gnoise2_f32(x: f32, y: f32, seed: u32) -> f32 {
    let ix = floor(x);
    let iy = floor(y);
    let fx = fract(x);
    let fy = fract(y);

    let a = __lp_lpfn_random2_f32(ix, iy, seed);
    let b = __lp_lpfn_random2_f32(ix + 1.0, iy, seed);
    let c = __lp_lpfn_random2_f32(ix, iy + 1.0, seed);
    let d = __lp_lpfn_random2_f32(ix + 1.0, iy + 1.0, seed);

    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);

    mix(a, b, ux) + (c - a) * uy * (1.0 - ux) + (d - b) * ux * uy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lattice_points_equal_the_underlying_random() {
        for i in -4..=4 {
            for j in -4..=4 {
                let (x, y) = (i as f32, j as f32);
                assert_eq!(
                    __lp_lpfn_gnoise2_f32(x, y, 0),
                    __lp_lpfn_random2_f32(x, y, 0)
                );
            }
        }
    }

    #[test]
    fn stays_inside_the_unit_interval() {
        for i in -40..=40 {
            for j in -40..=40 {
                let v = __lp_lpfn_gnoise2_f32(i as f32 * 0.19, j as f32 * 0.23, 5);
                assert!((0.0..=1.0).contains(&v), "gnoise2 = {v}");
            }
        }
    }
}
