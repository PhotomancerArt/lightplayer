//! 2D Worley noise, value variant (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/worley/worley2_value.glsl` (normative). Same
//! nearest-feature search as `lpfn_worley` (noise-rs lineage, MIT/Apache-2.0 —
//! see docs/reports/2026-03-31-lpfx-license-audit.md), but returns a per-cell
//! hash value in approximately `[-1, 1]` instead of the distance.
//!
//! **Tolerance:** value Worley is **discontinuous at cell-ownership
//! boundaries** — an arbitrarily small coordinate change can flip which cell
//! owns the point and jump the output across its whole range. So the
//! conformance harness allows a small fraction of boundary-flip outliers, and
//! no test here asserts continuity.

use super::worley2_f32::{worley2_cells, worley2_test};
use crate::builtins::lpfn::hash::lpfn_hash2;

/// 2D Worley value-noise function (float version).
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_worley_value(vec2 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_worley2_value_f32(x: f32, y: f32, seed: u32) -> f32 {
    let p = [x, y];
    let (near_x, near_y, far_x, far_y, _, _, range_x, range_y) = worley2_cells(x, y);

    let mut dist = worley2_test(p, seed, near_x, near_y);
    let mut seed_cell = (near_x, near_y);

    // Closure rather than three inlined blocks: the last candidate's distance
    // write is dead on its own, and inlining it makes the compiler say so —
    // but dropping the write would leave the three branches asymmetric and the
    // next reader guessing whether it was deliberate.
    let consider = |dist: &mut f32, cell: &mut (i32, i32), c: (i32, i32)| {
        let test = worley2_test(p, seed, c.0, c.1);
        if test < *dist {
            *dist = test;
            *cell = c;
        }
    };

    if range_x < dist {
        consider(&mut dist, &mut seed_cell, (far_x, near_y));
    }
    if range_y < dist {
        consider(&mut dist, &mut seed_cell, (near_x, far_y));
    }
    if range_x < dist && range_y < dist {
        consider(&mut dist, &mut seed_cell, (far_x, far_y));
    }

    // Hash the owning cell, normalize low byte to [0, 1], map to [-1, 1].
    let hash_value = lpfn_hash2(seed_cell.0 as u32, seed_cell.1 as u32, seed);
    let normalized = (hash_value & 0xFF) as f32 / 255.0;
    normalized * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_and_deterministic() {
        for i in -40..=40 {
            for j in -40..=40 {
                let (x, y) = (i as f32 * 0.19, j as f32 * 0.27);
                let v = __lp_lpfn_worley2_value_f32(x, y, 2);
                assert!((-1.0..=1.0).contains(&v), "worley2_value = {v}");
                assert_eq!(v, __lp_lpfn_worley2_value_f32(x, y, 2));
            }
        }
    }

    #[test]
    fn output_is_quantized_to_the_hash_low_byte() {
        // 256 possible values, evenly spaced — a useful invariant because it
        // catches a wrong normalization constant that a range check would not.
        let v = __lp_lpfn_worley2_value_f32(1.3, 2.7, 0);
        let step = 2.0 / 255.0;
        let k = ((v + 1.0) / step).round();
        assert!(((v + 1.0) - k * step).abs() < 1e-5, "{v} is off-grid");
    }
}
