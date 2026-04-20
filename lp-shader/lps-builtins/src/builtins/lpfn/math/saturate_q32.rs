//! Saturate function - clamp values between 0 and 1.
//!
//! This function clamps values to the [0, 1] range, which is commonly used in color
//! space conversions and other graphics operations.
//!
//! LICENSE: The saturate operation (clamp to [0,1]) is standard mathematical
//! procedure with no licensing concerns. While we may have referenced existing
//! implementations, the operation itself is trivial and not copyrightable.

use lps_q32::q32::Q32;
use lps_q32::vec3_q32::Vec3Q32;
use lps_q32::vec4_q32::Vec4Q32;

/// Saturate a single Q32 value (clamp to [0, 1]).
///
/// # Arguments
/// * `value` - Value to saturate
///
/// # Returns
/// Value clamped between 0 and 1
#[inline(always)]
pub fn lpfn_saturate_q32(value: Q32) -> Q32 {
    value.clamp(Q32::ZERO, Q32::ONE)
}

/// Saturate each component of a Vec3Q32 (clamp to [0, 1]).
///
/// # Arguments
/// * `v` - Vector to saturate
///
/// # Returns
/// Vector with each component clamped between 0 and 1
#[inline(always)]
pub fn lpfn_saturate_vec3_q32(v: Vec3Q32) -> Vec3Q32 {
    Vec3Q32::new(
        lpfn_saturate_q32(v.x),
        lpfn_saturate_q32(v.y),
        lpfn_saturate_q32(v.z),
    )
}

/// Saturate each component of a Vec4Q32 (clamp to [0, 1]).
///
/// # Arguments
/// * `v` - Vector to saturate
///
/// # Returns
/// Vector with each component clamped between 0 and 1
#[inline(always)]
pub fn lpfn_saturate_vec4_q32(v: Vec4Q32) -> Vec4Q32 {
    Vec4Q32::new(
        lpfn_saturate_q32(v.x),
        lpfn_saturate_q32(v.y),
        lpfn_saturate_q32(v.z),
        lpfn_saturate_q32(v.w),
    )
}

/// Saturate function for Q32 (extern C wrapper for compiler).
///
/// # Arguments
/// * `value` - Value to saturate as i32 (Q32 fixed-point)
///
/// # Returns
/// Value clamped between 0 and 1 as i32 (Q32 fixed-point)
#[lpfn_impl_macro::lpfn_impl(q32, "float lpfn_saturate(float x)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_saturate_q32(value: i32) -> i32 {
    lpfn_saturate_q32(Q32::from_fixed(value)).to_fixed()
}

/// Saturate function for vec3 (extern C wrapper for compiler).
///
/// Uses result pointer parameter to return vec3: writes all components to memory.
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where vec3 result will be written (result pointer parameter)
/// * `x` - X component as i32 (Q32 fixed-point)
/// * `y` - Y component as i32 (Q32 fixed-point)
/// * `z` - Z component as i32 (Q32 fixed-point)
#[lpfn_impl_macro::lpfn_impl(q32, "vec3 lpfn_saturate(vec3 v)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_saturate_vec3_q32(result_ptr: *mut i32, x: i32, y: i32, z: i32) {
    // Convert raw pointer to safe array reference at boundary
    let result = unsafe { &mut *result_ptr.cast::<[i32; 3]>() };
    let v = Vec3Q32::new(Q32::from_fixed(x), Q32::from_fixed(y), Q32::from_fixed(z));
    let saturated = lpfn_saturate_vec3_q32(v);
    result[0] = saturated.x.to_fixed();
    result[1] = saturated.y.to_fixed();
    result[2] = saturated.z.to_fixed();
}

/// Saturate function for vec4 (extern C wrapper for compiler).
///
/// Uses result pointer parameter to return vec4: writes all components to memory.
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where vec4 result will be written (result pointer parameter)
/// * `x` - X component as i32 (Q32 fixed-point)
/// * `y` - Y component as i32 (Q32 fixed-point)
/// * `z` - Z component as i32 (Q32 fixed-point)
/// * `w` - W component as i32 (Q32 fixed-point)
#[lpfn_impl_macro::lpfn_impl(q32, "vec4 lpfn_saturate(vec4 v)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_saturate_vec4_q32(
    result_ptr: *mut i32,
    x: i32,
    y: i32,
    z: i32,
    w: i32,
) {
    // Convert raw pointer to safe array reference at boundary
    let result = unsafe { &mut *result_ptr.cast::<[i32; 4]>() };
    let v = Vec4Q32::new(
        Q32::from_fixed(x),
        Q32::from_fixed(y),
        Q32::from_fixed(z),
        Q32::from_fixed(w),
    );
    let saturated = lpfn_saturate_vec4_q32(v);
    result[0] = saturated.x.to_fixed();
    result[1] = saturated.y.to_fixed();
    result[2] = saturated.z.to_fixed();
    result[3] = saturated.w.to_fixed();
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::fixed_to_float;

    #[test]
    fn test_saturate_q32_below_zero() {
        let result = lpfn_saturate_q32(Q32::from_f32_wrapping(-0.5));
        assert_eq!(result, Q32::ZERO, "Negative values should clamp to 0");
    }

    #[test]
    fn test_saturate_q32_above_one() {
        let result = lpfn_saturate_q32(Q32::from_f32_wrapping(1.5));
        assert_eq!(result, Q32::ONE, "Values above 1 should clamp to 1");
    }

    #[test]
    fn test_saturate_q32_in_range() {
        let result = lpfn_saturate_q32(Q32::from_f32_wrapping(0.5));
        let result_float = fixed_to_float(result.to_fixed());
        assert!(
            (result_float - 0.5).abs() < 0.0001,
            "Values in range should remain unchanged"
        );
    }

    #[test]
    fn test_saturate_q32_zero() {
        let result = lpfn_saturate_q32(Q32::ZERO);
        assert_eq!(result, Q32::ZERO, "Zero should remain zero");
    }

    #[test]
    fn test_saturate_q32_one() {
        let result = lpfn_saturate_q32(Q32::ONE);
        assert_eq!(result, Q32::ONE, "One should remain one");
    }

    #[test]
    fn test_saturate_vec3_q32() {
        let v = Vec3Q32::from_f32(-0.5, 0.5, 1.5);
        let result = lpfn_saturate_vec3_q32(v);
        assert_eq!(result.x, Q32::ZERO, "X component should clamp to 0");
        let y_float = fixed_to_float(result.y.to_fixed());
        assert!(
            (y_float - 0.5).abs() < 0.0001,
            "Y component should remain 0.5"
        );
        assert_eq!(result.z, Q32::ONE, "Z component should clamp to 1");
    }

    #[test]
    fn test_saturate_vec4_q32() {
        let v = Vec4Q32::from_f32(-0.5, 0.5, 1.5, 0.25);
        let result = lpfn_saturate_vec4_q32(v);
        assert_eq!(result.x, Q32::ZERO, "X component should clamp to 0");
        let y_float = fixed_to_float(result.y.to_fixed());
        assert!(
            (y_float - 0.5).abs() < 0.0001,
            "Y component should remain 0.5"
        );
        assert_eq!(result.z, Q32::ONE, "Z component should clamp to 1");
        let w_float = fixed_to_float(result.w.to_fixed());
        assert!(
            (w_float - 0.25).abs() < 0.0001,
            "W component should remain 0.25"
        );
    }
}
