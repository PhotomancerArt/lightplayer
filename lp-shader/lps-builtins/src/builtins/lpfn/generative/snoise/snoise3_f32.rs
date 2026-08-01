//! 3D simplex noise (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/snoise/snoise3.glsl` (normative).
//!
//! LightPlayer's snoise family is a structural rewrite of simplex noise:
//! gradient selection uses the integer `lpfn_hash` (noiz lineage, MIT) + a
//! 32-entry gradient LUT (12 edge gradients duplicated + 8 corner gradients)
//! instead of the mod-289 float permute hashing of the stegu/LYGIA original.
//! Simplex geometry and falloff follow Stefan Gustavson & Ian McEwan's simplex
//! noise (MIT, <https://github.com/stegu/webgl-noise>) via noise-rs.
//! See docs/reports/2026-03-31-lpfx-license-audit.md.
//!
//! **Tolerance:** exact against the canonical f32.

use crate::builtins::lpfn::hash::lpfn_hash3;
use crate::f32_math::floor;

const D: f32 = core::f32::consts::FRAC_1_SQRT_2;
/// 1/sqrt(3) — no `core` constant for this one.
const E: f32 = 0.577_350_26;

/// 32-entry gradient LUT: 0-11 edge gradients, 12-23 duplicates,
/// 24-31 corner gradients.
#[inline(always)]
fn grad(index: u32) -> [f32; 3] {
    let mut i = index % 32;
    if (12..24).contains(&i) {
        i -= 12; // duplicated edge gradients
    }
    match i {
        0 => [D, D, 0.0],
        1 => [-D, D, 0.0],
        2 => [D, -D, 0.0],
        3 => [-D, -D, 0.0],
        4 => [D, 0.0, D],
        5 => [-D, 0.0, D],
        6 => [D, 0.0, -D],
        7 => [-D, 0.0, -D],
        8 => [0.0, D, D],
        9 => [0.0, -D, D],
        10 => [0.0, D, -D],
        11 => [0.0, -D, -D],
        24 => [E, E, E],
        25 => [-E, E, E],
        26 => [E, -E, E],
        27 => [-E, -E, E],
        28 => [E, E, -E],
        29 => [-E, E, -E],
        30 => [E, -E, -E],
        _ => [-E, -E, -E],
    }
}

/// Surflet contribution: `t = 1 - 2|off|^2`, falloff `2t^2 + t^4`.
#[inline(always)]
fn surflet(gi: u32, off: [f32; 3]) -> f32 {
    let d2 = off[0] * off[0] + off[1] * off[1] + off[2] * off[2];
    let t = 1.0 - 2.0 * d2;
    if t > 0.0 {
        let g = grad(gi);
        let t2 = t * t;
        let falloff = 2.0 * t2 + t2 * t2;
        (g[0] * off[0] + g[1] * off[1] + g[2] * off[2]) * falloff
    } else {
        0.0
    }
}

/// 3D Simplex Noise function (float version).
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_snoise(vec3 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_snoise3_f32(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    const SKEW: f32 = 1.0 / 3.0;
    const UNSKEW: f32 = 1.0 / 6.0;

    // Skew input space to determine the simplex cell.
    let s = (x + y + z) * SKEW;
    let cx = floor(x + s) as i32;
    let cy = floor(y + s) as i32;
    let cz = floor(z + s) as i32;

    // Unskew the cell origin back to input space.
    let u = (cx + cy + cz) as f32 * UNSKEW;
    let off1 = [
        x - (cx as f32 - u),
        y - (cy as f32 - u),
        z - (cz as f32 - u),
    ];

    // Rank-order the offsets to pick the traversal order. The branch structure
    // is the canonical's, tie-breaking included — reordering these comparisons
    // moves cell boundaries.
    let (order1, order2): ([f32; 3], [f32; 3]) = if off1[0] >= off1[1] {
        if off1[1] >= off1[2] {
            ([1.0, 0.0, 0.0], [1.0, 1.0, 0.0]) // X Y Z
        } else if off1[0] >= off1[2] {
            ([1.0, 0.0, 0.0], [1.0, 0.0, 1.0]) // X Z Y
        } else {
            ([0.0, 0.0, 1.0], [1.0, 0.0, 1.0]) // Z X Y
        }
    } else if off1[1] < off1[2] {
        ([0.0, 0.0, 1.0], [0.0, 1.0, 1.0]) // Z Y X
    } else if off1[0] < off1[2] {
        ([0.0, 1.0, 0.0], [0.0, 1.0, 1.0]) // Y Z X
    } else {
        ([0.0, 1.0, 0.0], [1.0, 1.0, 0.0]) // Y X Z
    };

    let off2 = [
        off1[0] - order1[0] + UNSKEW,
        off1[1] - order1[1] + UNSKEW,
        off1[2] - order1[2] + UNSKEW,
    ];
    let off3 = [
        off1[0] - order2[0] + 2.0 * UNSKEW,
        off1[1] - order2[1] + 2.0 * UNSKEW,
        off1[2] - order2[2] + 2.0 * UNSKEW,
    ];
    let off4 = [
        off1[0] - 1.0 + 3.0 * UNSKEW,
        off1[1] - 1.0 + 3.0 * UNSKEW,
        off1[2] - 1.0 + 3.0 * UNSKEW,
    ];

    let gi0 = lpfn_hash3(cx as u32, cy as u32, cz as u32, seed);
    let gi1 = lpfn_hash3(
        (cx + order1[0] as i32) as u32,
        (cy + order1[1] as i32) as u32,
        (cz + order1[2] as i32) as u32,
        seed,
    );
    let gi2 = lpfn_hash3(
        (cx + order2[0] as i32) as u32,
        (cy + order2[1] as i32) as u32,
        (cz + order2[2] as i32) as u32,
        seed,
    );
    let gi3 = lpfn_hash3((cx + 1) as u32, (cy + 1) as u32, (cz + 1) as u32, seed);

    surflet(gi0, off1) + surflet(gi1, off2) + surflet(gi2, off3) + surflet(gi3, off4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gradient_lut_is_unit_length_everywhere() {
        for i in 0..32u32 {
            let g = grad(i);
            let len2 = g[0] * g[0] + g[1] * g[1] + g[2] * g[2];
            assert!((len2 - 1.0).abs() < 1e-6, "grad({i}) length^2 = {len2}");
        }
    }

    #[test]
    fn the_duplicated_edge_gradients_really_are_duplicates() {
        for i in 0..12u32 {
            assert_eq!(grad(i), grad(i + 12), "index {i}");
        }
    }

    #[test]
    fn bounded_and_deterministic() {
        for i in -15..=15 {
            for j in -15..=15 {
                for k in -3..=3 {
                    let (x, y, z) = (i as f32 * 0.29, j as f32 * 0.31, k as f32 * 0.7);
                    let v = __lp_lpfn_snoise3_f32(x, y, z, 6);
                    assert!((-1.5..=1.5).contains(&v), "snoise3 = {v}");
                    assert_eq!(v, __lp_lpfn_snoise3_f32(x, y, z, 6));
                }
            }
        }
    }
}
