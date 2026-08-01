//! 3D signed random returning vec3 (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/srandom/srandom3_vec.glsl` (normative).
//!
//! Three independent sin-hash channels with the classic gradient-noise dot
//! constants (127.1/311.7/74.7 family). The constants and sin-hash pattern are
//! the widely used public one-liner (David Hoskins lineage, MIT; see
//! docs/reports/2026-03-31-lpfx-license-audit.md).
//!
//! NOTE: the seed parameter is accepted but unused, matching the canonical and
//! the Q32 implementation; seeding this variant is future work tracked with the
//! Q32 implementation.
//!
//! **Tolerance:** chaotic sin-hash — statistical, not pointwise.

use crate::builtins::lpfn::generative::random::random1_f32::SIN_HASH_K3;
use crate::f32_math::fract;

/// 3D signed random vec3 (float version).
///
/// # Arguments
/// * `out` - Pointer to memory where the vec3 result is written
/// * `x` / `y` / `z` - Input coordinate as f32
/// * `seed` - Accepted for signature compatibility; unused (see module docs)
#[lpfn_impl_macro::lpfn_impl(f32, "vec3 lpfn_srandom3_vec(vec3 p, uint seed)")]
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes to caller-provided out-pointer"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_srandom3_vec_f32(out: *mut f32, x: f32, y: f32, z: f32, seed: u32) {
    let _ = seed;
    let v = srandom3_vec(x, y, z);
    unsafe {
        *out = v[0];
        *out.add(1) = v[1];
        *out.add(2) = v[2];
    }
}

/// Rust-facing form; the tiling and gradient-noise builtins call this directly.
#[inline]
pub(crate) fn srandom3_vec(x: f32, y: f32, z: f32) -> [f32; 3] {
    let dx = x * 127.1 + y * 311.7 + z * 74.7;
    let dy = x * 269.5 + y * 183.3 + z * 246.1;
    let dz = x * 113.5 + y * 271.9 + z * 124.6;
    [
        -1.0 + 2.0 * fract(libm::sinf(dx) * SIN_HASH_K3),
        -1.0 + 2.0 * fract(libm::sinf(dy) * SIN_HASH_K3),
        -1.0 + 2.0 * fract(libm::sinf(dz) * SIN_HASH_K3),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_lane_is_signed_and_bounded() {
        for i in -30..=30 {
            let t = i as f32 * 0.41;
            let v = srandom3_vec(t, t * 1.3, t * 0.7);
            for c in v {
                assert!((-1.0..1.0).contains(&c), "lane {c} at {t}");
            }
        }
    }

    #[test]
    fn the_three_lanes_are_independent() {
        let v = srandom3_vec(1.0, 2.0, 3.0);
        assert_ne!(v[0], v[1]);
        assert_ne!(v[1], v[2]);
    }

    #[test]
    fn writes_all_three_out_lanes() {
        let mut out = [f32::NAN; 3];
        __lp_lpfn_srandom3_vec_f32(out.as_mut_ptr(), 1.0, 2.0, 3.0, 0);
        assert_eq!(out, srandom3_vec(1.0, 2.0, 3.0));
    }
}
