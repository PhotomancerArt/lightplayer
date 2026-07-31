//! 2D sin-based random (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/random/random2.glsl` (normative — see
//! `docs/adr/2026-07-08-glsl-canonical-builtins.md`).
//!
//! `fract(sin(dot(p, K)) * 43758.5453)`. Credit: MIT License (MIT)
//! Copyright 2014, David Hoskins; distributed by LYGIA in
//! generative/random.glsl (see docs/reports/2026-03-31-lpfx-license-audit.md).
//!
//! Seed semantics: raw seed word added to the angle = `seed * 2^-16` radians of
//! phase, matching the Q32 implementation (see `random1_f32`).
//!
//! **Tolerance:** chaotic sin-hash — conformance is statistical, not pointwise.
//! See `random1_f32` for the full reasoning.

use super::random1_f32::{SIN_HASH_K, seed_phase, sin_hash};

/// 2D Random function (float version).
///
/// # Arguments
/// * `x` - X coordinate as f32
/// * `y` - Y coordinate as f32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Random value in [0, 1) range as f32
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_random(vec2 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_random2_f32(x: f32, y: f32, seed: u32) -> f32 {
    let d = x * 12.9898 + y * 78.233;
    sin_hash(d + seed_phase(seed), SIN_HASH_K)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_is_always_in_the_unit_interval() {
        for i in -60..=60 {
            for j in -60..=60 {
                let v = __lp_lpfn_random2_f32(i as f32 * 0.31, j as f32 * 0.47, 0);
                assert!((0.0..1.0).contains(&v), "random2({i},{j}) = {v}");
            }
        }
    }

    #[test]
    fn the_axes_are_not_interchangeable() {
        assert_ne!(
            __lp_lpfn_random2_f32(1.0, 2.0, 0),
            __lp_lpfn_random2_f32(2.0, 1.0, 0)
        );
    }
}
