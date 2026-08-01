//! 2D simplex noise (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/snoise/snoise2.glsl` (normative).
//!
//! LightPlayer's snoise family is a structural rewrite of simplex noise:
//! gradient selection uses the integer `lpfn_hash` (noiz lineage, MIT) + an
//! 8-entry gradient LUT instead of the mod-289 float permute hashing of the
//! stegu/LYGIA original. The skew/unskew simplex geometry and radial falloff
//! follow Stefan Gustavson & Ian McEwan's simplex noise (MIT,
//! <https://github.com/stegu/webgl-noise>) via the noise-rs library.
//! See docs/reports/2026-03-31-lpfx-license-audit.md.
//!
//! **Tolerance:** exact against the canonical f32 — integer hashing plus
//! polynomial falloff, no chaotic amplification.

use crate::builtins::lpfn::hash::lpfn_hash2;
use crate::f32_math::floor;

/// 1/sqrt(2).
const D: f32 = core::f32::consts::FRAC_1_SQRT_2;

/// 8-entry gradient LUT: 4 axis-aligned + 4 diagonal.
#[inline(always)]
fn grad(index: u32) -> [f32; 2] {
    match index & 7 {
        0 => [1.0, 0.0],
        1 => [-1.0, 0.0],
        2 => [0.0, 1.0],
        3 => [0.0, -1.0],
        4 => [D, D],
        5 => [-D, D],
        6 => [D, -D],
        _ => [-D, -D],
    }
}

/// Surflet contribution: `t = 1 - 2|off|^2`, falloff `2t^2 + t^4`.
#[inline(always)]
fn surflet(gi: u32, off: [f32; 2]) -> f32 {
    let t = 1.0 - 2.0 * (off[0] * off[0] + off[1] * off[1]);
    if t > 0.0 {
        let g = grad(gi);
        let t2 = t * t;
        let falloff = 2.0 * t2 + t2 * t2;
        (g[0] * off[0] + g[1] * off[1]) * falloff
    } else {
        0.0
    }
}

/// 2D Simplex Noise function (float version).
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_snoise(vec2 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_snoise2_f32(x: f32, y: f32, seed: u32) -> f32 {
    const SKEW: f32 = 0.366_025_4; // (sqrt(3) - 1) / 2
    const UNSKEW: f32 = 0.211_324_87; // (3 - sqrt(3)) / 6

    // Skew input space to determine the simplex cell.
    let s = (x + y) * SKEW;
    let cx = floor(x + s) as i32;
    let cy = floor(y + s) as i32;

    // Unskew the cell origin back to input space.
    let u = (cx + cy) as f32 * UNSKEW;
    let origin = [cx as f32 - u, cy as f32 - u];

    // Offsets from the three simplex corners.
    let off1 = [x - origin[0], y - origin[1]];
    // Middle corner: (1,0) if x-major, (0,1) if y-major (ties go x-major).
    let cmp = if off1[0] >= off1[1] { 1.0 } else { 0.0 };
    let order = [cmp, 1.0 - cmp];
    let off2 = [off1[0] - order[0] + UNSKEW, off1[1] - order[1] + UNSKEW];
    let off3 = [off1[0] - 1.0 + 2.0 * UNSKEW, off1[1] - 1.0 + 2.0 * UNSKEW];

    // Gradient indices from the integer hash of each corner.
    let gi0 = lpfn_hash2(cx as u32, cy as u32, seed);
    let gi1 = lpfn_hash2(
        (cx + order[0] as i32) as u32,
        (cy + order[1] as i32) as u32,
        seed,
    );
    let gi2 = lpfn_hash2((cx + 1) as u32, (cy + 1) as u32, seed);

    surflet(gi0, off1) + surflet(gi1, off2) + surflet(gi2, off3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_and_deterministic() {
        for i in -40..=40 {
            for j in -40..=40 {
                let (x, y) = (i as f32 * 0.17, j as f32 * 0.23);
                let v = __lp_lpfn_snoise2_f32(x, y, 4);
                assert!((-1.5..=1.5).contains(&v), "snoise2({x},{y}) = {v}");
                assert_eq!(v, __lp_lpfn_snoise2_f32(x, y, 4));
            }
        }
    }

    #[test]
    fn the_gradient_lut_is_unit_length() {
        for i in 0..8u32 {
            let g = grad(i);
            let len2 = g[0] * g[0] + g[1] * g[1];
            assert!((len2 - 1.0).abs() < 1e-6, "grad({i}) length^2 = {len2}");
        }
    }

    #[test]
    fn the_seed_changes_the_field() {
        assert_ne!(
            __lp_lpfn_snoise2_f32(1.3, 2.7, 0),
            __lp_lpfn_snoise2_f32(1.3, 2.7, 1)
        );
    }
}
