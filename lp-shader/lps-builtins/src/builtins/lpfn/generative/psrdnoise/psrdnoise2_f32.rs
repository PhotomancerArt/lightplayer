//! 2D tiling simplex flow noise with rotating gradients and analytic
//! derivative (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/psrdnoise/psrdnoise2.glsl` (normative).
//!
//! Original algorithm and implementation:
//! Copyright 2021-2023 by Stefan Gustavson and Ian McEwan
//! (<https://github.com/stegu/psrdnoise>), distributed by LYGIA in
//! generative/psrdnoise.glsl. Published under the MIT license:
//! <https://opensource.org/license/mit/>
//! This LightPlayer port replaces the float mod-289 permute with an exact
//! integer mod-289 permutation (same values where both are exact) and keeps the
//! rest of the algorithm.
//!
//! The seed parameter is accepted but unused, matching the canonical and the
//! Q32 implementation (psrdnoise derives its permutation from the lattice
//! coordinates only).
//!
//! **Tolerance:** exact against the canonical f32 — the permutation is exact
//! integer arithmetic and the rest is polynomial plus `sin`/`cos` for the
//! gradient angles.

use crate::f32_math::{floor, fract, glsl_mod};

/// Integer corner hash; values stay in `[0, 288]`.
///
/// Rust's `%` is the truncated remainder, so a negative intermediate needs the
/// explicit `+= 289` the canonical also performs. This is not defensive
/// programming — negative lattice coordinates are ordinary here.
#[inline]
fn hash(iu: i32, iv: i32) -> i32 {
    let mut h = iu % 289;
    if h < 0 {
        h += 289;
    }
    h = ((h * 51 + 2) * h + iv) % 289;
    if h < 0 {
        h += 289;
    }
    h = ((h * 34 + 10) * h) % 289;
    if h < 0 {
        h += 289;
    }
    h
}

/// 2D Periodic Simplex Rotational Domain noise function (float version).
///
/// # Arguments
/// * `x` / `y` - Coordinate as f32
/// * `period_x` / `period_y` - Period per axis (0 = no tiling on that axis)
/// * `alpha` - Gradient rotation angle in radians
/// * `gradient_out` - Pointer to output gradient `[gx, gy]`
/// * `seed` - Accepted for signature compatibility; unused (see module docs)
///
/// # Returns
/// Noise value approximately in range [-1, 1] as f32
#[lpfn_impl_macro::lpfn_impl(
    f32,
    "float lpfn_psrdnoise(vec2 x, vec2 period, float alpha, out vec2 gradient, uint seed)"
)]
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes gradient through caller-provided out-pointer"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_psrdnoise2_f32(
    x: f32,
    y: f32,
    period_x: f32,
    period_y: f32,
    alpha: f32,
    gradient_out: *mut f32,
    seed: u32,
) -> f32 {
    let _ = seed;

    // Transform to simplex space (skewed grid).
    let uv = [x + y * 0.5, y];
    let i0f = [floor(uv[0]), floor(uv[1])];
    let f0 = [fract(uv[0]), fract(uv[1])];

    // cmp = step(f0.y, f0.x): 1 if f0.x >= f0.y else 0.
    let cmp = if f0[0] >= f0[1] { 1.0f32 } else { 0.0 };
    let o1 = [cmp, 1.0 - cmp];

    let i1f = [i0f[0] + o1[0], i0f[1] + o1[1]];
    let i2f = [i0f[0] + 1.0, i0f[1] + 1.0];

    // Transform corners back to input space.
    let v0 = [i0f[0] - i0f[1] * 0.5, i0f[1]];
    let v1 = [v0[0] + o1[0] - o1[1] * 0.5, v0[1] + o1[1]];
    let v2 = [v0[0] + 0.5, v0[1] + 1.0];

    let x0 = [x - v0[0], y - v0[1]];
    let x1 = [x - v1[0], y - v1[1]];
    let x2 = [x - v2[0], y - v2[1]];

    // Corner indices for hashing: wrapped when tiling, raw otherwise.
    let (iu, iv): ([i32; 3], [i32; 3]) = if period_x > 0.0 || period_y > 0.0 {
        let mut xw = [v0[0], v1[0], v2[0]];
        let mut yw = [v0[1], v1[1], v2[1]];
        if period_x > 0.0 {
            for c in &mut xw {
                *c = glsl_mod(*c, period_x);
            }
        }
        if period_y > 0.0 {
            for c in &mut yw {
                *c = glsl_mod(*c, period_y);
            }
        }
        (
            [
                floor(xw[0] + yw[0] * 0.5 + 0.5) as i32,
                floor(xw[1] + yw[1] * 0.5 + 0.5) as i32,
                floor(xw[2] + yw[2] * 0.5 + 0.5) as i32,
            ],
            [
                floor(yw[0] + 0.5) as i32,
                floor(yw[1] + 0.5) as i32,
                floor(yw[2] + 0.5) as i32,
            ],
        )
    } else {
        (
            [i0f[0] as i32, i1f[0] as i32, i2f[0] as i32],
            [i0f[1] as i32, i1f[1] as i32, i2f[1] as i32],
        )
    };

    // Gradients: unit vectors at angle psi = hash * 0.07482 rotated by alpha.
    let mut g = [[0.0f32; 2]; 3];
    for k in 0..3 {
        let psi = hash(iu[k], iv[k]) as f32 * 0.07482;
        g[k] = [libm::cosf(psi + alpha), libm::sinf(psi + alpha)];
    }

    let xs = [x0, x1, x2];

    // Radial decay: w = max(0.8 - |x_k|^2, 0).
    let mut w = [0.0f32; 3];
    let mut gdotx = [0.0f32; 3];
    for k in 0..3 {
        let d2 = xs[k][0] * xs[k][0] + xs[k][1] * xs[k][1];
        w[k] = (0.8 - d2).max(0.0);
        gdotx[k] = g[k][0] * xs[k][0] + g[k][1] * xs[k][1];
    }

    let mut n = 0.0f32;
    let mut grad = [0.0f32; 2];
    for k in 0..3 {
        let w2 = w[k] * w[k];
        let w3 = w2 * w[k];
        let w4 = w2 * w2;
        n += w4 * gdotx[k];
        // Analytic derivative.
        let dw = -8.0 * w3 * gdotx[k];
        grad[0] += g[k][0] * w4 + xs[k][0] * dw;
        grad[1] += g[k][1] * w4 + xs[k][1] * dw;
    }

    unsafe {
        *gradient_out = 10.9 * grad[0];
        *gradient_out.add(1) = 10.9 * grad[1];
    }

    10.9 * n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(x: f32, y: f32, px: f32, py: f32, alpha: f32) -> (f32, [f32; 2]) {
        let mut g = [0.0f32; 2];
        let n = __lp_lpfn_psrdnoise2_f32(x, y, px, py, alpha, g.as_mut_ptr(), 0);
        (n, g)
    }

    #[test]
    fn bounded_and_deterministic() {
        for i in -30..=30 {
            for j in -30..=30 {
                let (x, y) = (i as f32 * 0.21, j as f32 * 0.29);
                let (n, g) = eval(x, y, 0.0, 0.0, 0.0);
                assert!(n.abs() <= 1.5, "psrdnoise2({x},{y}) = {n}");
                assert!(g[0].is_finite() && g[1].is_finite());
                assert_eq!(n, eval(x, y, 0.0, 0.0, 0.0).0);
            }
        }
    }

    #[test]
    fn it_tiles_when_a_period_is_given() {
        let p = 4.0f32;
        for i in 0..8 {
            let t = i as f32 * 0.5;
            let a = eval(t, t, p, p, 0.0).0;
            let b = eval(t + p, t + p, p, p, 0.0).0;
            assert!((a - b).abs() < 1e-4, "at {t}: {a} vs {b}");
        }
    }

    #[test]
    fn the_gradient_matches_a_numeric_derivative() {
        // The analytic derivative is the easiest thing to get subtly wrong in
        // this transliteration, and nothing else would notice.
        let h = 1e-3f32;
        for (x, y) in [(0.3f32, 0.7f32), (1.9, -2.4), (-0.6, 0.15)] {
            let (_, g) = eval(x, y, 0.0, 0.0, 0.0);
            let dx =
                (eval(x + h, y, 0.0, 0.0, 0.0).0 - eval(x - h, y, 0.0, 0.0, 0.0).0) / (2.0 * h);
            let dy =
                (eval(x, y + h, 0.0, 0.0, 0.0).0 - eval(x, y - h, 0.0, 0.0, 0.0).0) / (2.0 * h);
            assert!(
                (g[0] - dx).abs() < 5e-2,
                "d/dx at ({x},{y}): {} vs {dx}",
                g[0]
            );
            assert!(
                (g[1] - dy).abs() < 5e-2,
                "d/dy at ({x},{y}): {} vs {dy}",
                g[1]
            );
        }
    }

    #[test]
    fn alpha_rotates_the_field() {
        let a = eval(0.3, 0.7, 0.0, 0.0, 0.0).0;
        let b = eval(0.3, 0.7, 0.0, 0.0, 1.0).0;
        assert_ne!(a, b);
    }
}
