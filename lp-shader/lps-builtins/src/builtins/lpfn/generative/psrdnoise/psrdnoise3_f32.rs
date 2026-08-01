//! 3D tiling simplex flow noise with rotating gradients and analytic
//! derivative (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/psrdnoise/psrdnoise3.glsl` (normative).
//!
//! Original algorithm and implementation:
//! Copyright 2021-2023 by Stefan Gustavson and Ian McEwan
//! (<https://github.com/stegu/psrdnoise>), distributed by LYGIA in
//! generative/psrdnoise.glsl. Published under the MIT license:
//! <https://opensource.org/license/mit/>
//! This LightPlayer port replaces the float mod-289 permute with an exact
//! integer mod-289 permutation (same values where both are exact); gradients
//! come from the Fibonacci-spiral sphere distribution of the original.
//!
//! **Follows the canonical, not the Q32 implementation:** the Q32 device code
//! tabulates the spiral in a 289-entry LUT; this computes it in closed form,
//! as the canonical specifies.
//!
//! The seed parameter is accepted but unused, matching the canonical and the
//! Q32 implementation.
//!
//! **Tolerance:** exact against the canonical f32.
//!
//! # Known defect inherited from the canonical: `period` does not tile
//!
//! On the periodic path the canonical recomputes the corner offsets from the
//! **wrapped** corner positions while leaving `x` itself unwrapped, so
//! evaluating at `x + period` gives offsets larger by `period`, which fall
//! outside the radial support and return `0`. The Q32 implementation does the
//! same thing (`psrdnoise3_q32.rs`, "Recompute x vectors from wrapped v"), and
//! the 2D sibling does **not** — `psrdnoise2` keeps the unwrapped offsets and
//! tiles correctly.
//!
//! This port reproduces the behavior deliberately: the canonical GLSL is
//! normative, the Q32 sibling agrees with it, and "improving" the f32 version
//! would make the two float modes disagree while leaving the actual bug in
//! place. Recorded in `docs/defects/2026-07-31-psrdnoise3-period-does-not-tile.md`;
//! fixing it is a canonical-plus-Q32 change, out of scope for the f32 family.

use crate::f32_math::{floor, fract, glsl_mod, sqrt};

/// Integer corner hash; values stay in `[0, 288]`.
/// Matches the canonical: `permute(permute(permute(iw) + iv) + iu)`.
#[inline]
fn hash(iu: i32, iv: i32, iw: i32) -> i32 {
    let mut h = iw % 289;
    if h < 0 {
        h += 289;
    }
    h = ((h * 34 + 1) * h) % 289;
    if h < 0 {
        h += 289;
    }
    let mut hv = (h + iv) % 289;
    if hv < 0 {
        hv += 289;
    }
    h *= hv * 34 + 1;
    h = (h + iu) % 289;
    if h < 0 {
        h += 289;
    }
    h = ((h * 34 + 10) * h) % 289;
    if h < 0 {
        h += 289;
    }
    h
}

/// Fibonacci-spiral constants, kept digit-for-digit from `psrdnoise3.glsl`.
/// `2*pi / golden ratio`, `1 - (2h + 0.5)/289` as slope+bias, and `10*pi / 289`.
#[allow(
    clippy::excessive_precision,
    reason = "canonical GLSL constants, kept digit-for-digit so the source and the port can be diffed; the f32 values are identical"
)]
const THETA_STEP: f32 = 3.883222077452858;
const SZ_SLOPE: f32 = -0.006920415;
#[allow(
    clippy::excessive_precision,
    reason = "canonical GLSL constant, kept digit-for-digit; the f32 value is identical"
)]
const SZ_BIAS: f32 = 0.996539792;
const PSI_STEP: f32 = 0.108705628;

/// Gradient for a corner: Fibonacci-spiral sphere point, psi-rotated, then
/// alpha-rotated about the tangent axis `q`.
#[inline]
fn grad(hash: i32, sin_alpha: f32, cos_alpha: f32) -> [f32; 3] {
    let h = hash as f32;

    let theta = h * THETA_STEP;
    let sz = h * SZ_SLOPE + SZ_BIAS;
    let psi = h * PSI_STEP;

    let ct = libm::cosf(theta);
    let st = libm::sinf(theta);
    let sz_prime = sqrt(1.0 - sz * sz);

    // Orthogonal tangent vector q and spiral point p.
    let q = [st, -ct];
    let p = [-sz * ct, -sz * st, sz_prime];

    // Base gradient after psi rotation: g_b = cos(psi)*p + sin(psi)*(q, 0).
    let cp = libm::cosf(psi);
    let sp = libm::sinf(psi);
    let gb = [cp * p[0] + sp * q[0], cp * p[1] + sp * q[1], cp * p[2]];

    // Alpha rotation about q (qz == 0).
    [
        cos_alpha * gb[0] + sin_alpha * q[0],
        cos_alpha * gb[1] + sin_alpha * q[1],
        cos_alpha * gb[2],
    ]
}

/// 3D Periodic Simplex Rotational Domain noise function (float version).
///
/// # Arguments
/// * `x` / `y` / `z` - Coordinate as f32
/// * `period_x` / `period_y` / `period_z` - Period per axis (0 = no tiling)
/// * `alpha` - Gradient rotation angle in radians
/// * `gradient_out` - Pointer to output gradient `[gx, gy, gz]`
/// * `seed` - Accepted for signature compatibility; unused (see module docs)
///
/// # Returns
/// Noise value approximately in range [-1, 1] as f32
#[lpfn_impl_macro::lpfn_impl(
    f32,
    "float lpfn_psrdnoise(vec3 x, vec3 period, float alpha, out vec3 gradient, uint seed)"
)]
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes gradient through caller-provided out-pointer"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_psrdnoise3_f32(
    x: f32,
    y: f32,
    z: f32,
    period_x: f32,
    period_y: f32,
    period_z: f32,
    alpha: f32,
    gradient_out: *mut f32,
    seed: u32,
) -> f32 {
    let _ = seed;

    let sin_alpha = libm::sinf(alpha);
    let cos_alpha = libm::cosf(alpha);

    // Transform to simplex space (tetrahedral grid): uvw = x + dot(x, 1/3).
    let dot_sum = (x + y + z) * (1.0 / 3.0);
    let uvw = [x + dot_sum, y + dot_sum, z + dot_sum];

    let i0 = [floor(uvw[0]), floor(uvw[1]), floor(uvw[2])];
    let f0 = [fract(uvw[0]), fract(uvw[1]), fract(uvw[2])];

    // Rank-order u, v, w to find the simplex traversal order.
    // g_ = step(f0.xyx, f0.yzz): 1 if f0.xyx <= f0.yzz.
    let step = |edge: f32, v: f32| if v < edge { 0.0f32 } else { 1.0 };
    let g_ = [step(f0[0], f0[1]), step(f0[1], f0[2]), step(f0[0], f0[2])];
    let l_ = [1.0 - g_[0], 1.0 - g_[1], 1.0 - g_[2]];
    let gg = [l_[2], g_[0], g_[1]];
    let ll = [l_[0], l_[1], g_[2]];
    let o1 = [gg[0].min(ll[0]), gg[1].min(ll[1]), gg[2].min(ll[2])];
    let o2 = [gg[0].max(ll[0]), gg[1].max(ll[1]), gg[2].max(ll[2])];

    let i1 = [i0[0] + o1[0], i0[1] + o1[1], i0[2] + o1[2]];
    let i2 = [i0[0] + o2[0], i0[1] + o2[1], i0[2] + o2[2]];
    let i3 = [i0[0] + 1.0, i0[1] + 1.0, i0[2] + 1.0];

    // Transform corners back to input space: v = i - dot(i, 1/6).
    let unskew = |i: [f32; 3]| {
        let d = (i[0] + i[1] + i[2]) * (1.0 / 6.0);
        [i[0] - d, i[1] - d, i[2] - d]
    };
    let corners = [unskew(i0), unskew(i1), unskew(i2), unskew(i3)];

    let mut idx = [[0i32; 3]; 4];
    let mut offs = [[0.0f32; 3]; 4];

    if period_x > 0.0 || period_y > 0.0 || period_z > 0.0 {
        let periods = [period_x, period_y, period_z];
        let mut wrapped = corners;
        for c in &mut wrapped {
            for a in 0..3 {
                if periods[a] > 0.0 {
                    c[a] = glsl_mod(c[a], periods[a]);
                }
            }
        }
        for k in 0..4 {
            // Transform wrapped corners back to uvw and round to the lattice.
            let dv = (wrapped[k][0] + wrapped[k][1] + wrapped[k][2]) * (1.0 / 3.0);
            for a in 0..3 {
                idx[k][a] = floor(wrapped[k][a] + dv + 0.5) as i32;
            }
            offs[k] = [x - wrapped[k][0], y - wrapped[k][1], z - wrapped[k][2]];
        }
    } else {
        let lattice = [i0, i1, i2, i3];
        for k in 0..4 {
            for a in 0..3 {
                idx[k][a] = lattice[k][a] as i32;
            }
            offs[k] = [x - corners[k][0], y - corners[k][1], z - corners[k][2]];
        }
    }

    let mut n = 0.0f32;
    let mut gradient = [0.0f32; 3];
    for k in 0..4 {
        let g = grad(hash(idx[k][0], idx[k][1], idx[k][2]), sin_alpha, cos_alpha);
        let off = offs[k];
        let d2 = off[0] * off[0] + off[1] * off[1] + off[2] * off[2];
        // Radial decay: w = max(0.5 - |x_k|^2, 0).
        let w = (0.5 - d2).max(0.0);
        let w2 = w * w;
        let w3 = w2 * w;
        let gdotx = g[0] * off[0] + g[1] * off[1] + g[2] * off[2];
        n += w3 * gdotx;
        // Analytic derivative.
        let dw = -6.0 * w2 * gdotx;
        for (a, gr) in gradient.iter_mut().enumerate() {
            *gr += g[a] * w3 + off[a] * dw;
        }
    }

    unsafe {
        for (a, gr) in gradient.iter().enumerate() {
            *gradient_out.add(a) = 39.5 * gr;
        }
    }

    39.5 * n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(x: f32, y: f32, z: f32, p: f32, alpha: f32) -> (f32, [f32; 3]) {
        let mut g = [0.0f32; 3];
        let n = __lp_lpfn_psrdnoise3_f32(x, y, z, p, p, p, alpha, g.as_mut_ptr(), 0);
        (n, g)
    }

    #[test]
    fn the_gradient_lut_is_unit_length() {
        for h in 0..289 {
            let g = grad(h, 0.0, 1.0);
            let len2 = g[0] * g[0] + g[1] * g[1] + g[2] * g[2];
            assert!((len2 - 1.0).abs() < 1e-3, "hash {h} length^2 = {len2}");
        }
    }

    #[test]
    fn bounded_and_deterministic() {
        for i in -12..=12 {
            for j in -12..=12 {
                for k in -3..=3 {
                    let (x, y, z) = (i as f32 * 0.27, j as f32 * 0.31, k as f32 * 0.53);
                    let (n, g) = eval(x, y, z, 0.0, 0.0);
                    assert!(n.abs() <= 1.5, "psrdnoise3 = {n}");
                    for c in g {
                        assert!(c.is_finite());
                    }
                    assert_eq!(n, eval(x, y, z, 0.0, 0.0).0);
                }
            }
        }
    }

    /// Pins the inherited defect rather than the intent: the periodic path
    /// does **not** repeat at the period, because the canonical takes the
    /// corner offsets from wrapped corners while `x` stays unwrapped (see the
    /// module docs). If someone fixes the canonical and the Q32 sibling, this
    /// test is the one that should start failing.
    #[test]
    fn the_periodic_path_does_not_actually_tile_yet() {
        let p = 3.0f32;
        let a = eval(0.4, 0.52, 0.28, p, 0.0).0;
        let b = eval(0.4 + p, 0.52 + p, 0.28 + p, p, 0.0).0;
        assert_ne!(
            a, b,
            "psrdnoise3 started tiling — if the canonical and psrdnoise3_q32 \
             were fixed together, delete this test and assert tiling instead"
        );
    }

    /// The periodic path still has to be finite and bounded, whatever it
    /// computes.
    #[test]
    fn the_periodic_path_stays_bounded() {
        for i in -10..=10 {
            let t = i as f32 * 0.3;
            let (n, g) = eval(t, t * 1.3, t * 0.7, 3.0, 0.0);
            assert!(n.is_finite() && n.abs() <= 1.5, "psrdnoise3 = {n}");
            for c in g {
                assert!(c.is_finite());
            }
        }
    }

    #[test]
    fn the_gradient_matches_a_numeric_derivative() {
        let h = 1e-3f32;
        for (x, y, z) in [(0.3f32, 0.7f32, 0.2f32), (1.9, -2.4, 0.8)] {
            let (_, g) = eval(x, y, z, 0.0, 0.0);
            let dx = (eval(x + h, y, z, 0.0, 0.0).0 - eval(x - h, y, z, 0.0, 0.0).0) / (2.0 * h);
            let dy = (eval(x, y + h, z, 0.0, 0.0).0 - eval(x, y - h, z, 0.0, 0.0).0) / (2.0 * h);
            let dz = (eval(x, y, z + h, 0.0, 0.0).0 - eval(x, y, z - h, 0.0, 0.0).0) / (2.0 * h);
            assert!((g[0] - dx).abs() < 5e-2, "d/dx: {} vs {dx}", g[0]);
            assert!((g[1] - dy).abs() < 5e-2, "d/dy: {} vs {dy}", g[1]);
            assert!((g[2] - dz).abs() < 5e-2, "d/dz: {} vs {dz}", g[2]);
        }
    }
}
