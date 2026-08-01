//! 3D value noise (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/gnoise/gnoise3.glsl` (normative): random values at the
//! eight cell corners, trilinear interpolation with quintic-smoothstep weights,
//! remapped from `[0, 1]` to `[-1, 1]`. Standard lattice value noise (see
//! docs/reports/2026-03-31-lpfx-license-audit.md).
//!
//! **Follows the canonical, not the Q32 implementation:** the Q32 device code
//! uses a 256-entry quintic LUT; this uses the exact `6t^5 - 15t^4 + 10t^3`.
//!
//! **Tolerance:** chaotic sin-hash underneath — statistical vs Q32.

use crate::builtins::lpfn::generative::random::random3_f32::__lp_lpfn_random3_f32;
use crate::f32_math::{floor, fract, mix};

/// Quintic smoothstep weight `6t^5 - 15t^4 + 10t^3`.
#[inline(always)]
pub(crate) fn quintic(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// 3D Gradient Noise function (float version). Returns `[-1, 1]`.
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_gnoise(vec3 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_gnoise3_f32(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let (ix, iy, iz) = (floor(x), floor(y), floor(z));
    let (ux, uy, uz) = (quintic(fract(x)), quintic(fract(y)), quintic(fract(z)));

    let c000 = __lp_lpfn_random3_f32(ix, iy, iz, seed);
    let c100 = __lp_lpfn_random3_f32(ix + 1.0, iy, iz, seed);
    let c010 = __lp_lpfn_random3_f32(ix, iy + 1.0, iz, seed);
    let c110 = __lp_lpfn_random3_f32(ix + 1.0, iy + 1.0, iz, seed);
    let c001 = __lp_lpfn_random3_f32(ix, iy, iz + 1.0, seed);
    let c101 = __lp_lpfn_random3_f32(ix + 1.0, iy, iz + 1.0, seed);
    let c011 = __lp_lpfn_random3_f32(ix, iy + 1.0, iz + 1.0, seed);
    let c111 = __lp_lpfn_random3_f32(ix + 1.0, iy + 1.0, iz + 1.0, seed);

    let x00 = mix(c000, c100, ux);
    let x10 = mix(c010, c110, ux);
    let x01 = mix(c001, c101, ux);
    let x11 = mix(c011, c111, ux);

    let y0 = mix(x00, x10, uy);
    let y1 = mix(x01, x11, uy);

    let result = mix(y0, y1, uz);

    // Remap [0, 1] -> [-1, 1].
    -1.0 + 2.0 * result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quintic_weight_has_the_right_endpoints_and_flat_derivative() {
        assert_eq!(quintic(0.0), 0.0);
        assert_eq!(quintic(1.0), 1.0);
        assert!((quintic(0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn output_is_signed_and_bounded() {
        for i in -12..=12 {
            for j in -12..=12 {
                for k in -3..=3 {
                    let v =
                        __lp_lpfn_gnoise3_f32(i as f32 * 0.37, j as f32 * 0.29, k as f32 * 0.61, 1);
                    assert!((-1.0..=1.0).contains(&v), "gnoise3 = {v}");
                }
            }
        }
    }
}
