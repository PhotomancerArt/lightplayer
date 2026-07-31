//! 3D Worley (cellular) noise, distance variant (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/worley/worley3.glsl` (normative). noise-rs
//! range-function lineage (MIT/Apache-2.0) with the integer `lpfn_hash` for
//! feature points — **NOT** LYGIA's Prosperity-licensed worley.glsl
//! (see docs/reports/2026-03-31-lpfx-license-audit.md).
//!
//! Returns the squared euclidean distance to the nearest feature point,
//! scaled/shifted to approximately `[-1, 1]`.
//!
//! **Tolerance:** exact against the canonical f32.

use crate::builtins::lpfn::hash::lpfn_hash3;
use crate::f32_math::floor;

/// Feature point for a cell: cell origin + hash-directed offset.
#[inline]
pub(crate) fn worley3_point(index: u32, cell_x: i32, cell_y: i32, cell_z: i32) -> [f32; 3] {
    // length in [0, 0.5] from bits 5-7 of the hash.
    let length_bits = ((index & 0xE0) >> 5) as f32;
    let len = length_bits * 0.5 / 7.0;
    let diag = len * core::f32::consts::FRAC_1_SQRT_2;

    let offset = match index % 18 {
        0 => [diag, diag, 0.0],
        1 => [diag, -diag, 0.0],
        2 => [-diag, diag, 0.0],
        3 => [-diag, -diag, 0.0],
        4 => [diag, 0.0, diag],
        5 => [diag, 0.0, -diag],
        6 => [-diag, 0.0, diag],
        7 => [-diag, 0.0, -diag],
        8 => [0.0, diag, diag],
        9 => [0.0, diag, -diag],
        10 => [0.0, -diag, diag],
        11 => [0.0, -diag, -diag],
        12 => [len, 0.0, 0.0],
        13 => [0.0, len, 0.0],
        14 => [0.0, 0.0, len],
        15 => [-len, 0.0, 0.0],
        16 => [0.0, -len, 0.0],
        _ => [0.0, 0.0, -len],
    };

    [
        cell_x as f32 + offset[0],
        cell_y as f32 + offset[1],
        cell_z as f32 + offset[2],
    ]
}

/// Squared distance from `p` to the feature point of cell `(tx, ty, tz)`.
#[inline]
pub(crate) fn worley3_test(p: [f32; 3], seed: u32, tx: i32, ty: i32, tz: i32) -> f32 {
    let index = lpfn_hash3(tx as u32, ty as u32, tz as u32, seed);
    let tp = worley3_point(index, tx, ty, tz);
    let d = [p[0] - tp[0], p[1] - tp[1], p[2] - tp[2]];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

/// Near/far cells and midline ranges per axis.
#[inline]
pub(crate) fn worley3_cells(x: f32, y: f32, z: f32) -> ([i32; 3], [i32; 3], [f32; 3]) {
    let cell = [floor(x) as i32, floor(y) as i32, floor(z) as i32];
    let frac = [x - cell[0] as f32, y - cell[1] as f32, z - cell[2] as f32];

    let mut near = [0i32; 3];
    let mut far = [0i32; 3];
    let mut range = [0f32; 3];
    for a in 0..3 {
        if frac[a] > 0.5 {
            near[a] = cell[a] + 1;
            far[a] = cell[a];
        } else {
            near[a] = cell[a];
            far[a] = cell[a] + 1;
        }
        range[a] = (0.5 - frac[a]) * (0.5 - frac[a]);
    }
    (near, far, range)
}

/// 3D Worley Noise function (float version).
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_worley(vec3 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_worley3_f32(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let p = [x, y, z];
    let (near, far, range) = worley3_cells(x, y, z);

    let mut dist = worley3_test(p, seed, near[0], near[1], near[2]);

    // Single-axis checks.
    if range[0] < dist {
        dist = dist.min(worley3_test(p, seed, far[0], near[1], near[2]));
    }
    if range[1] < dist {
        dist = dist.min(worley3_test(p, seed, near[0], far[1], near[2]));
    }
    if range[2] < dist {
        dist = dist.min(worley3_test(p, seed, near[0], near[1], far[2]));
    }

    // Two-axis checks.
    if range[0] < dist && range[1] < dist {
        dist = dist.min(worley3_test(p, seed, far[0], far[1], near[2]));
    }
    if range[0] < dist && range[2] < dist {
        dist = dist.min(worley3_test(p, seed, far[0], near[1], far[2]));
    }
    if range[1] < dist && range[2] < dist {
        dist = dist.min(worley3_test(p, seed, near[0], far[1], far[2]));
    }

    // Three-axis check.
    if range[0] < dist && range[1] < dist && range[2] < dist {
        dist = dist.min(worley3_test(p, seed, far[0], far[1], far[2]));
    }

    // Map to approximately [-1, 1] (matches the canonical's
    // (dist / 3) * 2 - 1 scaling).
    (dist / 3.0) * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_points_stay_within_half_a_cell() {
        for index in 0..256u32 {
            let p = worley3_point(index, 0, 0, 0);
            for c in p {
                assert!(c.abs() <= 0.5 + 1e-6, "offset {c} for index {index}");
            }
        }
    }

    #[test]
    fn bounded_and_deterministic() {
        for i in -12..=12 {
            for j in -12..=12 {
                for k in -3..=3 {
                    let (x, y, z) = (i as f32 * 0.23, j as f32 * 0.31, k as f32 * 0.61);
                    let v = __lp_lpfn_worley3_f32(x, y, z, 3);
                    assert!((-1.0..=1.0).contains(&v), "worley3 = {v}");
                    assert_eq!(v, __lp_lpfn_worley3_f32(x, y, z, 3));
                }
            }
        }
    }
}
