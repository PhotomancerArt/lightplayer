//! Saturate — clamp to `[0, 1]` (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/math/saturate.glsl`, which is normative
//! (`docs/adr/2026-07-08-glsl-canonical-builtins.md`).
//!
//! **Tolerance:** exact. `clamp` introduces no rounding.
//!
//! LICENSE: The saturate operation (clamp to [0,1]) is standard mathematical
//! procedure with no licensing concerns. The operation itself is trivial.

use crate::f32_math::clamp;

/// Saturate function for f32 (extern C wrapper for compiler).
///
/// # Arguments
/// * `value` - Value to saturate as f32
///
/// # Returns
/// Value clamped between 0 and 1 as f32
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_saturate(float x)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_saturate_f32(value: f32) -> f32 {
    clamp(value, 0.0, 1.0)
}

/// Saturate function for vec3 (extern C wrapper for compiler).
///
/// Uses result pointer parameter to return vec3: writes all components to memory.
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where vec3 result will be written (result pointer parameter)
/// * `x` - X component as f32
/// * `y` - Y component as f32
/// * `z` - Z component as f32
#[lpfn_impl_macro::lpfn_impl(f32, "vec3 lpfn_saturate(vec3 v)")]
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes to caller-provided out-pointer"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_saturate_vec3_f32(result_ptr: *mut f32, x: f32, y: f32, z: f32) {
    unsafe {
        *result_ptr = clamp(x, 0.0, 1.0);
        *result_ptr.add(1) = clamp(y, 0.0, 1.0);
        *result_ptr.add(2) = clamp(z, 0.0, 1.0);
    }
}

/// Saturate function for vec4 (extern C wrapper for compiler).
///
/// Uses result pointer parameter to return vec4: writes all components to memory.
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where vec4 result will be written (result pointer parameter)
/// * `x` - X component as f32
/// * `y` - Y component as f32
/// * `z` - Z component as f32
/// * `w` - W component as f32
#[lpfn_impl_macro::lpfn_impl(f32, "vec4 lpfn_saturate(vec4 v)")]
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes to caller-provided out-pointer"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_saturate_vec4_f32(
    result_ptr: *mut f32,
    x: f32,
    y: f32,
    z: f32,
    w: f32,
) {
    unsafe {
        *result_ptr = clamp(x, 0.0, 1.0);
        *result_ptr.add(1) = clamp(y, 0.0, 1.0);
        *result_ptr.add(2) = clamp(z, 0.0, 1.0);
        *result_ptr.add(3) = clamp(w, 0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_the_scalar_form() {
        assert_eq!(__lp_lpfn_saturate_f32(-1.0), 0.0);
        assert_eq!(__lp_lpfn_saturate_f32(0.25), 0.25);
        assert_eq!(__lp_lpfn_saturate_f32(2.0), 1.0);
    }

    #[test]
    fn range_far_outside_q32_still_clamps_rather_than_wrapping() {
        // The Q32 stub round-tripped through a wrapping conversion, so a large
        // input came back as garbage instead of 1.0. That is the defect this
        // whole family exists to remove.
        assert_eq!(__lp_lpfn_saturate_f32(1.0e30), 1.0);
        assert_eq!(__lp_lpfn_saturate_f32(-1.0e30), 0.0);
    }

    #[test]
    fn writes_every_vec3_lane() {
        let mut out = [-9.0f32; 3];
        __lp_lpfn_saturate_vec3_f32(out.as_mut_ptr(), -0.5, 0.5, 1.5);
        assert_eq!(out, [0.0, 0.5, 1.0]);
    }

    #[test]
    fn writes_every_vec4_lane() {
        let mut out = [-9.0f32; 4];
        __lp_lpfn_saturate_vec4_f32(out.as_mut_ptr(), -0.5, 0.25, 1.5, 0.75);
        assert_eq!(out, [0.0, 0.25, 1.0, 0.75]);
    }
}
