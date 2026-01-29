//! Convert HSV color space to RGB.
//!
//! Converts colors from HSV (Hue, Saturation, Value) color space to RGB color space.
//! This implementation follows the algorithm from lygia.

use crate::builtins::lpfx::color::space::hue2rgb_q32::lpfx_hue2rgb_q32;
use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;
use crate::glsl::q32::types::vec4_q32::Vec4Q32;

/// Convert HSV color to RGB color.
///
/// Converts a color from HSV color space to RGB color space.
///
/// # Arguments
/// * `hsv` - HSV color as Vec3Q32 (H, S, V components in range [0, 1])
///
/// # Returns
/// RGB color as Vec3Q32 with components in range [0, 1]
#[inline(always)]
pub fn lpfx_hsv2rgb_q32(hsv: Vec3Q32) -> Vec3Q32 {
    // Algorithm from lygia: ((hue2rgb(hsv.x) - 1.0) * hsv.y + 1.0) * hsv.z
    let hue_rgb = lpfx_hue2rgb_q32(hsv.x);
    let rgb_minus_one = hue_rgb - Vec3Q32::one();
    let rgb_scaled = rgb_minus_one * hsv.y + Vec3Q32::one();
    rgb_scaled * hsv.z
}

/// Convert HSV color to RGB color (with alpha channel preserved).
///
/// Converts a color from HSV color space to RGB color space, preserving
/// the alpha channel.
///
/// # Arguments
/// * `hsv` - HSV color as Vec4Q32 (H, S, V, A components, H/S/V in range [0, 1])
///
/// # Returns
/// RGBA color as Vec4Q32 with RGB components in range [0, 1], alpha preserved
#[inline(always)]
pub fn lpfx_hsv2rgb_vec4_q32(hsv: Vec4Q32) -> Vec4Q32 {
    let hsv_vec3 = Vec3Q32::new(hsv.x, hsv.y, hsv.z);
    let rgb_vec3 = lpfx_hsv2rgb_q32(hsv_vec3);
    Vec4Q32::new(rgb_vec3.x, rgb_vec3.y, rgb_vec3.z, hsv.w)
}

/// Convert HSV color to RGB color (extern C wrapper for compiler).
///
/// Uses result pointer parameter to return vec3: writes all components to memory.
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where vec3 result will be written (result pointer parameter)
/// * `x` - H component as i32 (Q32 fixed-point)
/// * `y` - S component as i32 (Q32 fixed-point)
/// * `z` - V component as i32 (Q32 fixed-point)
#[lpfx_impl_macro::lpfx_impl(q32, "vec3 lpfx_hsv2rgb(vec3 hsv)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_hsv2rgb_q32(result_ptr: *mut i32, x: i32, y: i32, z: i32) {
    // Convert raw pointer to safe array reference at boundary
    let result = unsafe { &mut *result_ptr.cast::<[i32; 3]>() };
    let hsv = Vec3Q32::new(Q32::from_fixed(x), Q32::from_fixed(y), Q32::from_fixed(z));
    let rgb = lpfx_hsv2rgb_q32(hsv);
    result[0] = rgb.x.to_fixed();
    result[1] = rgb.y.to_fixed();
    result[2] = rgb.z.to_fixed();
}

/// Convert HSV color to RGB color with alpha (extern C wrapper for compiler).
///
/// Uses result pointer parameter to return vec4: writes all components to memory.
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where vec4 result will be written (result pointer parameter)
/// * `x` - H component as i32 (Q32 fixed-point)
/// * `y` - S component as i32 (Q32 fixed-point)
/// * `z` - V component as i32 (Q32 fixed-point)
/// * `w` - A component as i32 (Q32 fixed-point)
#[lpfx_impl_macro::lpfx_impl(q32, "vec4 lpfx_hsv2rgb(vec4 hsv)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_hsv2rgb_vec4_q32(result_ptr: *mut i32, x: i32, y: i32, z: i32, w: i32) {
    // Convert raw pointer to safe array reference at boundary
    let result = unsafe { &mut *result_ptr.cast::<[i32; 4]>() };
    let hsv = Vec4Q32::new(
        Q32::from_fixed(x),
        Q32::from_fixed(y),
        Q32::from_fixed(z),
        Q32::from_fixed(w),
    );
    let rgb = lpfx_hsv2rgb_vec4_q32(hsv);
    result[0] = rgb.x.to_fixed();
    result[1] = rgb.y.to_fixed();
    result[2] = rgb.z.to_fixed();
    result[3] = rgb.w.to_fixed();
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::builtins::lpfx::color::space::rgb2hsv_q32::lpfx_rgb2hsv_q32;
    use crate::util::test_helpers::fixed_to_float;
    use std::vec;

    #[test]
    fn test_hsv2rgb_pure_red() {
        // HSV(0, 1, 1) -> RGB(1, 0, 0)
        let hsv = Vec3Q32::new(Q32::ZERO, Q32::ONE, Q32::ONE);
        let rgb = lpfx_hsv2rgb_q32(hsv);
        let r = fixed_to_float(rgb.x.to_fixed());
        let g = fixed_to_float(rgb.y.to_fixed());
        let b = fixed_to_float(rgb.z.to_fixed());
        assert!((r - 1.0).abs() < 0.01, "R should be ~1.0, got {}", r);
        assert!(g < 0.01, "G should be ~0.0, got {}", g);
        assert!(b < 0.01, "B should be ~0.0, got {}", b);
    }

    #[test]
    fn test_hsv2rgb_black() {
        // HSV(0, 0, 0) -> RGB(0, 0, 0)
        let hsv = Vec3Q32::zero();
        let rgb = lpfx_hsv2rgb_q32(hsv);
        assert_eq!(rgb.x, Q32::ZERO);
        assert_eq!(rgb.y, Q32::ZERO);
        assert_eq!(rgb.z, Q32::ZERO);
    }

    #[test]
    fn test_hsv2rgb_white() {
        // HSV(0, 0, 1) -> RGB(1, 1, 1)
        let hsv = Vec3Q32::new(Q32::ZERO, Q32::ZERO, Q32::ONE);
        let rgb = lpfx_hsv2rgb_q32(hsv);
        assert_eq!(rgb.x, Q32::ONE);
        assert_eq!(rgb.y, Q32::ONE);
        assert_eq!(rgb.z, Q32::ONE);
    }

    #[test]
    fn test_hsv2rgb_round_trip() {
        // RGB -> HSV -> RGB should be approximately equal
        let test_colors = vec![
            Vec3Q32::new(Q32::ONE, Q32::ZERO, Q32::ZERO), // Red
            Vec3Q32::new(Q32::ZERO, Q32::ONE, Q32::ZERO), // Green
            Vec3Q32::new(Q32::ZERO, Q32::ZERO, Q32::ONE), // Blue
            Vec3Q32::from_f32(0.5, 0.3, 0.8),
            Vec3Q32::from_f32(0.2, 0.7, 0.4),
        ];

        for rgb_original in test_colors {
            let hsv = lpfx_rgb2hsv_q32(rgb_original);
            let rgb_roundtrip = lpfx_hsv2rgb_q32(hsv);

            let r_diff = fixed_to_float((rgb_original.x - rgb_roundtrip.x).to_fixed()).abs();
            let g_diff = fixed_to_float((rgb_original.y - rgb_roundtrip.y).to_fixed()).abs();
            let b_diff = fixed_to_float((rgb_original.z - rgb_roundtrip.z).to_fixed()).abs();

            assert!(
                r_diff < 0.05 && g_diff < 0.05 && b_diff < 0.05,
                "Round-trip error too large: original {:?}, roundtrip {:?}",
                rgb_original,
                rgb_roundtrip
            );
        }
    }

    #[test]
    fn test_hsv2rgb_vec4_preserves_alpha() {
        let hsv = Vec4Q32::new(Q32::ZERO, Q32::ONE, Q32::ONE, Q32::from_f32(0.5));
        let rgb = lpfx_hsv2rgb_vec4_q32(hsv);
        let alpha = fixed_to_float(rgb.w.to_fixed());
        assert!((alpha - 0.5).abs() < 0.01, "Alpha should be preserved");
    }
}
