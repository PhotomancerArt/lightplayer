//! 3D sin-based random (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/random/random3.glsl` (normative).
//!
//! `fract(sin(dot(p, K)) * 43758.5453123)`. Credit: David Hoskins (MIT) via
//! LYGIA generative/random.glsl
//! (see docs/reports/2026-03-31-lpfx-license-audit.md).
//!
//! Note the multiplier differs from the 1D/2D variants (`...5453123` rather
//! than `...5453`); that is the canonical constant, not a typo, and changing it
//! would change every 3D noise pattern in the library.
//!
//! **Tolerance:** chaotic sin-hash — statistical, not pointwise. See
//! `random1_f32`.

use super::random1_f32::{SIN_HASH_K3, seed_phase, sin_hash};

/// 3D Random function (float version).
///
/// # Arguments
/// * `x` - X coordinate as f32
/// * `y` - Y coordinate as f32
/// * `z` - Z coordinate as f32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Random value in [0, 1) range as f32
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_random(vec3 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_random3_f32(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let d = x * 70.9898 + y * 78.233 + z * 32.4355;
    sin_hash(d + seed_phase(seed), SIN_HASH_K3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_is_always_in_the_unit_interval() {
        for i in -25..=25 {
            for j in -25..=25 {
                for k in -3..=3 {
                    let v =
                        __lp_lpfn_random3_f32(i as f32 * 0.31, j as f32 * 0.47, k as f32 * 1.7, 0);
                    assert!((0.0..1.0).contains(&v), "random3 = {v}");
                }
            }
        }
    }

    #[test]
    fn every_axis_matters() {
        let base = __lp_lpfn_random3_f32(1.0, 2.0, 3.0, 0);
        assert_ne!(base, __lp_lpfn_random3_f32(2.0, 2.0, 3.0, 0));
        assert_ne!(base, __lp_lpfn_random3_f32(1.0, 3.0, 3.0, 0));
        assert_ne!(base, __lp_lpfn_random3_f32(1.0, 2.0, 4.0, 0));
    }
}
