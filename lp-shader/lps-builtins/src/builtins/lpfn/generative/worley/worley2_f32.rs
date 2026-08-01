//! 2D Worley (cellular) noise, distance variant (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/worley/worley2.glsl` (normative).
//!
//! LightPlayer's worley is derived from the noise-rs library's range-function
//! optimization (MIT/Apache-2.0, <https://github.com/Razaekel/noise-rs>) of
//! Steven Worley's 1996 algorithm, using the integer `lpfn_hash` for feature
//! points — **NOT** LYGIA's Prosperity-licensed worley.glsl
//! (see docs/reports/2026-03-31-lpfx-license-audit.md).
//!
//! Returns the squared euclidean distance to the nearest feature point,
//! shifted to approximately `[-1, 1]`.
//!
//! **Tolerance:** exact against the canonical f32 — integer hashing and
//! polynomial distance only.

use crate::builtins::lpfn::hash::lpfn_hash2;
use crate::f32_math::floor;

/// Feature point for a cell: cell origin + hash-directed offset.
#[inline]
pub(crate) fn worley2_point(index: u32, cell_x: i32, cell_y: i32) -> [f32; 2] {
    // length in [0, 0.5] from bits 3-7 of the hash.
    let length_bits = ((index & 0xF8) >> 3) as f32;
    let len = length_bits * 0.5 / 31.0;
    let diag = len * core::f32::consts::FRAC_1_SQRT_2;

    let offset = match index & 0x07 {
        0 => [diag, diag],
        1 => [diag, -diag],
        2 => [-diag, diag],
        3 => [-diag, -diag],
        4 => [len, 0.0],
        5 => [-len, 0.0],
        6 => [0.0, len],
        _ => [0.0, -len],
    };

    [cell_x as f32 + offset[0], cell_y as f32 + offset[1]]
}

/// Squared distance from `p` to the feature point of cell `(tx, ty)`.
#[inline]
pub(crate) fn worley2_test(p: [f32; 2], seed: u32, tx: i32, ty: i32) -> f32 {
    let index = lpfn_hash2(tx as u32, ty as u32, seed);
    let tp = worley2_point(index, tx, ty);
    let dx = p[0] - tp[0];
    let dy = p[1] - tp[1];
    dx * dx + dy * dy
}

/// The shared near/far cell setup both 2D worley variants use.
#[inline]
pub(crate) fn worley2_cells(x: f32, y: f32) -> (i32, i32, i32, i32, i32, i32, f32, f32) {
    let cell_x = floor(x) as i32;
    let cell_y = floor(y) as i32;
    let frac_x = x - cell_x as f32;
    let frac_y = y - cell_y as f32;

    let near_x = if frac_x > 0.5 { cell_x + 1 } else { cell_x };
    let near_y = if frac_y > 0.5 { cell_y + 1 } else { cell_y };
    let far_x = if frac_x > 0.5 { cell_x } else { cell_x + 1 };
    let far_y = if frac_y > 0.5 { cell_y } else { cell_y + 1 };

    // Range test values: squared distance to the cell midlines.
    let range_x = (0.5 - frac_x) * (0.5 - frac_x);
    let range_y = (0.5 - frac_y) * (0.5 - frac_y);

    (
        near_x, near_y, far_x, far_y, cell_x, cell_y, range_x, range_y,
    )
}

/// 2D Worley Noise function (float version).
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_worley(vec2 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_worley2_f32(x: f32, y: f32, seed: u32) -> f32 {
    let p = [x, y];
    let (near_x, near_y, far_x, far_y, _, _, range_x, range_y) = worley2_cells(x, y);

    let mut dist = worley2_test(p, seed, near_x, near_y);

    if range_x < dist {
        dist = dist.min(worley2_test(p, seed, far_x, near_y));
    }
    if range_y < dist {
        dist = dist.min(worley2_test(p, seed, near_x, far_y));
    }
    if range_x < dist && range_y < dist {
        dist = dist.min(worley2_test(p, seed, far_x, far_y));
    }

    // Map to approximately [-1, 1] (matches the canonical's
    // (dist / 2) * 2 - 1 scaling).
    (dist / 2.0) * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_points_stay_within_half_a_cell() {
        for index in 0..256u32 {
            let p = worley2_point(index, 0, 0);
            assert!(p[0].abs() <= 0.5 + 1e-6, "x offset {}", p[0]);
            assert!(p[1].abs() <= 0.5 + 1e-6, "y offset {}", p[1]);
        }
    }

    #[test]
    fn bounded_and_deterministic() {
        for i in -40..=40 {
            for j in -40..=40 {
                let (x, y) = (i as f32 * 0.19, j as f32 * 0.27);
                let v = __lp_lpfn_worley2_f32(x, y, 2);
                assert!((-1.0..=1.0).contains(&v), "worley2({x},{y}) = {v}");
                assert_eq!(v, __lp_lpfn_worley2_f32(x, y, 2));
            }
        }
    }
}
