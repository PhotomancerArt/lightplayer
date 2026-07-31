//! 1D sin-based random (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/random/random1.glsl`, which is normative
//! (`docs/adr/2026-07-08-glsl-canonical-builtins.md`).
//!
//! Classic sin-hash: `fract(sin(x) * 43758.5453)`. Credit: David Hoskins /
//! the widely used one-liner distributed by LYGIA under MIT for random.glsl
//! (see docs/reports/2026-03-31-lpfx-license-audit.md).
//!
//! Seed semantics (LightPlayer): the Q32 implementation adds the raw seed word
//! to the Q16.16 angle, i.e. the phase shifts by `seed * 2^-16` radians. This
//! reproduces that exactly — `seed as f32 / 65536.0` — so a shader keeps the
//! same seed→pattern relationship in both float modes.
//!
//! **Tolerance:** this function is **chaotic by construction** (fract of a
//! large multiple of sin), so it has no pointwise tolerance versus the Q32
//! implementation and never will: a 1-ulp difference in the angle becomes an
//! O(1) difference in the output after the ×43758 amplification. Conformance
//! for the whole random family is **statistical** — the tests check range,
//! determinism and decorrelation, not agreement with Q32.
//!
//! Against the *canonical f32* definition, which is what
//! `docs/design/float.md` §6 actually requires, this is exact: the same
//! operations in the same order.

use crate::f32_math::fract;

/// The canonical sin-hash multiplier, kept digit-for-digit from
/// `random1.glsl` so the port can be diffed against its source.
#[allow(
    clippy::excessive_precision,
    reason = "canonical GLSL constant, kept digit-for-digit so the source and the port can be diffed; the f32 value is identical"
)]
pub(crate) const SIN_HASH_K: f32 = 43758.5453;

/// The 3D family uses a longer constant — also canonical, also verbatim.
#[allow(
    clippy::excessive_precision,
    reason = "canonical GLSL constant, kept digit-for-digit so the source and the port can be diffed; the f32 value is identical"
)]
pub(crate) const SIN_HASH_K3: f32 = 43758.5453123;

/// Seed → radians of phase. `1/65536` is exact in f32, so this conversion
/// introduces no error of its own.
#[inline(always)]
pub(crate) fn seed_phase(seed: u32) -> f32 {
    seed as f32 * (1.0 / 65536.0)
}

/// The shared sin-hash tail: `fract(sin(angle) * k)`.
#[inline(always)]
pub(crate) fn sin_hash(angle: f32, k: f32) -> f32 {
    fract(libm::sinf(angle) * k)
}

/// 1D Random function (float version).
///
/// # Arguments
/// * `x` - X coordinate as f32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Random value in [0, 1) range as f32
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_random(float x, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_random1_f32(x: f32, seed: u32) -> f32 {
    let combined = x + seed_phase(seed);
    sin_hash(combined, SIN_HASH_K)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_is_always_in_the_unit_interval() {
        for i in -500..=500 {
            let v = __lp_lpfn_random1_f32(i as f32 * 0.37, 0);
            assert!((0.0..1.0).contains(&v), "random1({i}) = {v}");
        }
    }

    #[test]
    fn deterministic() {
        assert_eq!(
            __lp_lpfn_random1_f32(1.25, 7),
            __lp_lpfn_random1_f32(1.25, 7)
        );
    }

    #[test]
    fn the_seed_decorrelates() {
        assert_ne!(
            __lp_lpfn_random1_f32(1.25, 0),
            __lp_lpfn_random1_f32(1.25, 1)
        );
    }

    /// The capability being bought: Q32 tops out near ±32768, so a large
    /// coordinate used to wrap into nonsense. f32 keeps hashing.
    #[test]
    fn large_coordinates_still_produce_a_unit_value() {
        for x in [1.0e5f32, 1.0e6, 1.0e7] {
            let v = __lp_lpfn_random1_f32(x, 0);
            assert!((0.0..1.0).contains(&v), "random1({x}) = {v}");
        }
    }
}
