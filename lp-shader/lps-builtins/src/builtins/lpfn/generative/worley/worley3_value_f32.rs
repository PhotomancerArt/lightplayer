//! 3D Worley noise, value variant (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/worley/worley3_value.glsl` (normative). Same
//! nearest-feature search as `lpfn_worley(vec3)` (noise-rs lineage,
//! MIT/Apache-2.0 — see docs/reports/2026-03-31-lpfx-license-audit.md), but
//! returns a per-cell hash value in approximately `[-1, 1]`.
//!
//! **Tolerance:** value Worley is discontinuous at cell-ownership boundaries;
//! the conformance harness allows a small fraction of boundary-flip outliers.

use super::worley3_f32::{worley3_cells, worley3_test};
use crate::builtins::lpfn::hash::lpfn_hash3;

/// 3D Worley value-noise function (float version).
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_worley_value(vec3 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_worley3_value_f32(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let p = [x, y, z];
    let (near, far, range) = worley3_cells(x, y, z);

    let mut dist = worley3_test(p, seed, near[0], near[1], near[2]);
    let mut cell = [near[0], near[1], near[2]];

    let consider = |dist: &mut f32, cell: &mut [i32; 3], c: [i32; 3]| {
        let test = worley3_test(p, seed, c[0], c[1], c[2]);
        if test < *dist {
            *dist = test;
            *cell = c;
        }
    };

    // Single-axis checks.
    if range[0] < dist {
        consider(&mut dist, &mut cell, [far[0], near[1], near[2]]);
    }
    if range[1] < dist {
        consider(&mut dist, &mut cell, [near[0], far[1], near[2]]);
    }
    if range[2] < dist {
        consider(&mut dist, &mut cell, [near[0], near[1], far[2]]);
    }

    // Two-axis checks.
    if range[0] < dist && range[1] < dist {
        consider(&mut dist, &mut cell, [far[0], far[1], near[2]]);
    }
    if range[0] < dist && range[2] < dist {
        consider(&mut dist, &mut cell, [far[0], near[1], far[2]]);
    }
    if range[1] < dist && range[2] < dist {
        consider(&mut dist, &mut cell, [near[0], far[1], far[2]]);
    }

    // Three-axis check.
    if range[0] < dist && range[1] < dist && range[2] < dist {
        consider(&mut dist, &mut cell, [far[0], far[1], far[2]]);
    }

    // Hash the owning cell, normalize low byte to [0, 1], map to [-1, 1].
    let hash_value = lpfn_hash3(cell[0] as u32, cell[1] as u32, cell[2] as u32, seed);
    let normalized = (hash_value & 0xFF) as f32 / 255.0;
    normalized * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_and_deterministic() {
        for i in -12..=12 {
            for j in -12..=12 {
                for k in -3..=3 {
                    let (x, y, z) = (i as f32 * 0.23, j as f32 * 0.31, k as f32 * 0.61);
                    let v = __lp_lpfn_worley3_value_f32(x, y, z, 3);
                    assert!((-1.0..=1.0).contains(&v), "worley3_value = {v}");
                    assert_eq!(v, __lp_lpfn_worley3_value_f32(x, y, z, 3));
                }
            }
        }
    }

    #[test]
    fn output_is_quantized_to_the_hash_low_byte() {
        let v = __lp_lpfn_worley3_value_f32(1.3, 2.7, 0.9, 0);
        let step = 2.0 / 255.0;
        let k = ((v + 1.0) / step).round();
        assert!(((v + 1.0) - k * step).abs() < 1e-5, "{v} is off-grid");
    }
}
