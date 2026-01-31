//! Convert RGB color space to HSV.
//!
//! Converts colors from RGB color space to HSV (Hue, Saturation, Value) color space.
//! This implementation follows Sam Hocevar's algorithm from lygia.

use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;
use crate::glsl::q32::types::vec4_q32::Vec4Q32;

/// Epsilon constant to avoid division by zero.
/// Using minimum representable Q32 value (1 in Q16.16 format = 1/65536 ≈ 0.000015).
const HCV_EPSILON_Q32: Q32 = Q32(1);

/// Fixed-point constants for rgb2hsv calculation
const SIX: Q32 = Q32(0x00060000); // 6.0 in Q16.16

/// K constant vector for rgb2hsv algorithm
/// K = vec4(0., -0.33333333333333333333, 0.6666666666666666666, -1.0)
const K_X: Q32 = Q32::ZERO;
const K_Y: Q32 = Q32::from_fixed(-21845); // -0.33333333333333333333 * 65536 ≈ -21845
const K_Z: Q32 = Q32::from_fixed(43690); // 0.6666666666666666666 * 65536 ≈ 43690
const K_W: Q32 = Q32::from_fixed(-65536); // -1.0 * 65536 = -65536

/// Convert RGB color to HSV color.
///
/// Converts a color from RGB color space to HSV color space.
/// Algorithm from Sam Hocevar: http://lolengine.net/blog/2013/07/27/rgb-to-hsv-in-glsl
///
/// # Arguments
/// * `rgb` - RGB color as Vec3Q32 with components in range [0, 1]
///
/// # Returns
/// HSV color as Vec3Q32 (H, S, V components in range [0, 1])
#[inline(always)]
pub fn lpfx_rgb2hsv_q32(rgb: Vec3Q32) -> Vec3Q32 {
    // Algorithm from lygia (Sam Hocevar's implementation)
    // vec4 K = vec4(0., -0.33333333333333333333, 0.6666666666666666666, -1.0);
    // vec4 p = c.g < c.b ? vec4(c.bg, K.wz) : vec4(c.gb, K.xy);
    // vec4 q = c.r < p.x ? vec4(p.xyw, c.r) : vec4(c.r, p.yzx);
    // float d = q.x - min(q.w, q.y);
    // return vec3(abs(q.z + (q.w - q.y) / (6. * d + HCV_EPSILON)),
    //             d / (q.x + HCV_EPSILON),
    //             q.x);

    let c = rgb;
    let p = if c.y < c.z {
        // p = vec4(c.bg, K.wz) = vec4(c.z, c.y, K_W, K_Z)
        Vec4Q32::new(c.z, c.y, K_W, K_Z)
    } else {
        // p = vec4(c.gb, K.xy) = vec4(c.y, c.z, K_X, K_Y)
        Vec4Q32::new(c.y, c.z, K_X, K_Y)
    };

    let q = if c.x < p.x {
        // q = vec4(p.xyw, c.r) = vec4(p.x, p.y, p.w, c.x)
        Vec4Q32::new(p.x, p.y, p.w, c.x)
    } else {
        // q = vec4(c.r, p.yzx) = vec4(c.x, p.y, p.z, p.x)
        Vec4Q32::new(c.x, p.y, p.z, p.x)
    };

    let d = q.x - q.w.min(q.y);
    let h = (q.z + (q.w - q.y) / (SIX * d + HCV_EPSILON_Q32)).abs();
    let s = d / (q.x + HCV_EPSILON_Q32);
    let v = q.x;

    Vec3Q32::new(h, s, v)
}

/// Convert RGB color to HSV color (with alpha channel preserved).
///
/// Converts a color from RGB color space to HSV color space, preserving
/// the alpha channel.
///
/// # Arguments
/// * `rgb` - RGBA color as Vec4Q32 with RGB components in range [0, 1]
///
/// # Returns
/// HSVA color as Vec4Q32 (H, S, V components in range [0, 1], alpha preserved)
#[inline(always)]
pub fn lpfx_rgb2hsv_vec4_q32(rgb: Vec4Q32) -> Vec4Q32 {
    let rgb_vec3 = Vec3Q32::new(rgb.x, rgb.y, rgb.z);
    let hsv_vec3 = lpfx_rgb2hsv_q32(rgb_vec3);
    Vec4Q32::new(hsv_vec3.x, hsv_vec3.y, hsv_vec3.z, rgb.w)
}

/// Convert RGB color to HSV color (extern C wrapper for compiler).
///
/// Uses result pointer parameter to return vec3: writes all components to memory.
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where vec3 result will be written (result pointer parameter)
/// * `x` - R component as i32 (Q32 fixed-point)
/// * `y` - G component as i32 (Q32 fixed-point)
/// * `z` - B component as i32 (Q32 fixed-point)
#[lpfx_impl_macro::lpfx_impl(q32, "vec3 lpfx_rgb2hsv(vec3 rgb)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_rgb2hsv_q32(result_ptr: *mut i32, x: i32, y: i32, z: i32) {
    // Convert raw pointer to safe array reference at boundary
    let result = unsafe { &mut *result_ptr.cast::<[i32; 3]>() };
    let rgb = Vec3Q32::new(Q32::from_fixed(x), Q32::from_fixed(y), Q32::from_fixed(z));
    let hsv = lpfx_rgb2hsv_q32(rgb);
    result[0] = hsv.x.to_fixed();
    result[1] = hsv.y.to_fixed();
    result[2] = hsv.z.to_fixed();
}

/// Convert RGB color to HSV color with alpha (extern C wrapper for compiler).
///
/// Uses result pointer parameter to return vec4: writes all components to memory.
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where vec4 result will be written (result pointer parameter)
/// * `x` - R component as i32 (Q32 fixed-point)
/// * `y` - G component as i32 (Q32 fixed-point)
/// * `z` - B component as i32 (Q32 fixed-point)
/// * `w` - A component as i32 (Q32 fixed-point)
#[lpfx_impl_macro::lpfx_impl(q32, "vec4 lpfx_rgb2hsv(vec4 rgb)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_rgb2hsv_vec4_q32(result_ptr: *mut i32, x: i32, y: i32, z: i32, w: i32) {
    // Convert raw pointer to safe array reference at boundary
    let result = unsafe { &mut *result_ptr.cast::<[i32; 4]>() };
    let rgb = Vec4Q32::new(
        Q32::from_fixed(x),
        Q32::from_fixed(y),
        Q32::from_fixed(z),
        Q32::from_fixed(w),
    );
    let hsv = lpfx_rgb2hsv_vec4_q32(rgb);
    result[0] = hsv.x.to_fixed();
    result[1] = hsv.y.to_fixed();
    result[2] = hsv.z.to_fixed();
    result[3] = hsv.w.to_fixed();
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::builtins::lpfx::color::space::hsv2rgb_q32::lpfx_hsv2rgb_q32;
    use crate::util::test_helpers::fixed_to_float;
    use std::vec;

    #[test]
    fn test_rgb2hsv_pure_red() {
        // RGB(1, 0, 0) -> HSV(0, 1, 1)
        let rgb = Vec3Q32::new(Q32::ONE, Q32::ZERO, Q32::ZERO);
        let hsv = lpfx_rgb2hsv_q32(rgb);
        let h = fixed_to_float(hsv.x.to_fixed());
        let s = fixed_to_float(hsv.y.to_fixed());
        let v = fixed_to_float(hsv.z.to_fixed());
        assert!(
            h < 0.01 || (h - 1.0).abs() < 0.01,
            "H should be ~0.0 or ~1.0, got {}",
            h
        );
        assert!((s - 1.0).abs() < 0.01, "S should be ~1.0, got {}", s);
        assert!((v - 1.0).abs() < 0.01, "V should be ~1.0, got {}", v);
    }

    #[test]
    fn test_rgb2hsv_black() {
        // RGB(0, 0, 0) -> HSV(0, 0, 0)
        let rgb = Vec3Q32::zero();
        let hsv = lpfx_rgb2hsv_q32(rgb);
        assert_eq!(hsv.x, Q32::ZERO);
        assert_eq!(hsv.y, Q32::ZERO);
        assert_eq!(hsv.z, Q32::ZERO);
    }

    #[test]
    fn test_rgb2hsv_white() {
        // RGB(1, 1, 1) -> HSV(0, 0, 1)
        let rgb = Vec3Q32::one();
        let hsv = lpfx_rgb2hsv_q32(rgb);
        assert_eq!(hsv.y, Q32::ZERO, "Saturation should be 0 for white");
        assert_eq!(hsv.z, Q32::ONE, "Value should be 1 for white");
    }

    #[test]
    fn test_rgb2hsv_grayscale() {
        // Grayscale colors should have saturation = 0
        for i in 1..10 {
            let gray = Q32::from_f32(i as f32 / 10.0);
            let rgb = Vec3Q32::new(gray, gray, gray);
            let hsv = lpfx_rgb2hsv_q32(rgb);
            let s = fixed_to_float(hsv.y.to_fixed());
            assert!(s < 0.01, "Grayscale should have saturation ~0, got {}", s);
        }
    }

    #[test]
    fn test_rgb2hsv_epsilon_case_nearly_equal() {
        // Test colors with very small differences between components (epsilon case)
        let test_cases = vec![
            Vec3Q32::from_f32(0.5, 0.50001, 0.5), // G slightly larger
            Vec3Q32::from_f32(0.5, 0.5, 0.50001), // B slightly larger
            Vec3Q32::from_f32(0.50001, 0.5, 0.5), // R slightly larger
            Vec3Q32::from_f32(0.1, 0.10001, 0.1), // Very small values
            Vec3Q32::from_f32(0.9, 0.90001, 0.9), // Very large values
        ];

        for rgb in test_cases {
            // Should not panic or produce invalid results
            let hsv = lpfx_rgb2hsv_q32(rgb);
            assert!(
                hsv.x >= Q32::ZERO && hsv.x <= Q32::ONE,
                "H should be in [0, 1]"
            );
            assert!(
                hsv.y >= Q32::ZERO && hsv.y <= Q32::ONE,
                "S should be in [0, 1]"
            );
            assert!(
                hsv.z >= Q32::ZERO && hsv.z <= Q32::ONE,
                "V should be in [0, 1]"
            );
        }
    }

    #[test]
    fn test_rgb2hsv_epsilon_case_one_dominates() {
        // Test colors where one component dominates (very small differences)
        let test_cases = vec![
            Vec3Q32::from_f32(1.0, 0.0001, 0.0001), // R dominates
            Vec3Q32::from_f32(0.0001, 1.0, 0.0001), // G dominates
            Vec3Q32::from_f32(0.0001, 0.0001, 1.0), // B dominates
        ];

        for rgb in test_cases {
            // Should not panic or produce invalid results
            let hsv = lpfx_rgb2hsv_q32(rgb);
            assert!(
                hsv.x >= Q32::ZERO && hsv.x <= Q32::ONE,
                "H should be in [0, 1]"
            );
            assert!(
                hsv.y >= Q32::ZERO && hsv.y <= Q32::ONE,
                "S should be in [0, 1]"
            );
            assert!(
                hsv.z >= Q32::ZERO && hsv.z <= Q32::ONE,
                "V should be in [0, 1]"
            );
        }
    }

    #[test]
    fn test_rgb2hsv_round_trip() {
        // HSV -> RGB -> HSV should be approximately equal
        let test_hsvs = vec![
            Vec3Q32::new(Q32::ZERO, Q32::ONE, Q32::ONE),
            Vec3Q32::from_f32(0.333, 1.0, 1.0),
            Vec3Q32::from_f32(0.666, 1.0, 1.0),
            Vec3Q32::from_f32(0.5, 0.7, 0.8),
            Vec3Q32::from_f32(0.2, 0.5, 0.6),
        ];

        for hsv_original in test_hsvs {
            let rgb = lpfx_hsv2rgb_q32(hsv_original);
            let hsv_roundtrip = lpfx_rgb2hsv_q32(rgb);

            let h_diff = fixed_to_float((hsv_original.x - hsv_roundtrip.x).to_fixed()).abs();
            let s_diff = fixed_to_float((hsv_original.y - hsv_roundtrip.y).to_fixed()).abs();
            let v_diff = fixed_to_float((hsv_original.z - hsv_roundtrip.z).to_fixed()).abs();

            // Allow larger tolerance for hue due to wrapping
            assert!(
                h_diff < 0.1 || (h_diff - 1.0).abs() < 0.1,
                "H round-trip error too large: original {}, roundtrip {}",
                fixed_to_float(hsv_original.x.to_fixed()),
                fixed_to_float(hsv_roundtrip.x.to_fixed())
            );
            assert!(
                s_diff < 0.05,
                "S round-trip error too large: original {}, roundtrip {}",
                fixed_to_float(hsv_original.y.to_fixed()),
                fixed_to_float(hsv_roundtrip.y.to_fixed())
            );
            assert!(
                v_diff < 0.05,
                "V round-trip error too large: original {}, roundtrip {}",
                fixed_to_float(hsv_original.z.to_fixed()),
                fixed_to_float(hsv_roundtrip.z.to_fixed())
            );
        }
    }

    #[test]
    fn test_rgb2hsv_range_validation() {
        // All HSV components should be in [0, 1]
        for i in 0..50 {
            let r = Q32::from_f32(i as f32 / 50.0);
            for j in 0..50 {
                let g = Q32::from_f32(j as f32 / 50.0);
                for k in 0..50 {
                    let b = Q32::from_f32(k as f32 / 50.0);
                    let rgb = Vec3Q32::new(r, g, b);
                    let hsv = lpfx_rgb2hsv_q32(rgb);

                    assert!(
                        hsv.x >= Q32::ZERO && hsv.x <= Q32::ONE,
                        "H should be in [0, 1]"
                    );
                    assert!(
                        hsv.y >= Q32::ZERO && hsv.y <= Q32::ONE,
                        "S should be in [0, 1]"
                    );
                    assert!(
                        hsv.z >= Q32::ZERO && hsv.z <= Q32::ONE,
                        "V should be in [0, 1]"
                    );
                }
            }
        }
    }

    #[test]
    fn test_rgb2hsv_vec4_preserves_alpha() {
        let rgb = Vec4Q32::new(Q32::ONE, Q32::ZERO, Q32::ZERO, Q32::from_f32(0.7));
        let hsv = lpfx_rgb2hsv_vec4_q32(rgb);
        let alpha = fixed_to_float(hsv.w.to_fixed());
        assert!((alpha - 0.7).abs() < 0.01, "Alpha should be preserved");
    }
}
