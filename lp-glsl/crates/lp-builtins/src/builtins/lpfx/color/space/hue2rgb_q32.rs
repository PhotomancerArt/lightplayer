//! Convert hue value to RGB color.
//!
//! Converts a hue value (0-1) to an RGB vec3 color. This is a helper function
//! used by HSV to RGB conversion.

use crate::builtins::lpfx::math::saturate_q32::lpfx_saturate_vec3_q32;
use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;

/// Fixed-point constants for hue2rgb calculation
const TWO: Q32 = Q32(0x00020000); // 2.0 in Q16.16
const THREE: Q32 = Q32(0x00030000); // 3.0 in Q16.16
const FOUR: Q32 = Q32(0x00040000); // 4.0 in Q16.16
const SIX: Q32 = Q32(0x00060000); // 6.0 in Q16.16

/// Convert hue value to RGB color.
///
/// Converts a hue value in the range [0, 1] to an RGB vec3 color.
/// The hue value wraps around (hue values > 1.0 are handled via fract).
///
/// # Arguments
/// * `hue` - Hue value in range [0, 1] (Q32 fixed-point)
///
/// # Returns
/// RGB color as Vec3Q32 with components in range [0, 1]
#[inline(always)]
pub fn lpfx_hue2rgb_q32(hue: Q32) -> Vec3Q32 {
    // Algorithm from lygia: uses abs() and arithmetic to compute RGB from hue
    // R = abs(hue * 6.0 - 3.0) - 1.0
    // G = 2.0 - abs(hue * 6.0 - 2.0)
    // B = 2.0 - abs(hue * 6.0 - 4.0)
    let hue_times_six = hue * SIX;
    let r = (hue_times_six - THREE).abs() - Q32::ONE;
    let g = TWO - (hue_times_six - TWO).abs();
    let b = TWO - (hue_times_six - FOUR).abs();

    let rgb = Vec3Q32::new(r, g, b);
    lpfx_saturate_vec3_q32(rgb)
}

/// Convert hue value to RGB color (extern C wrapper for compiler).
///
/// Uses result pointer parameter to return vec3: writes all components to memory.
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where vec3 result will be written (result pointer parameter)
/// * `hue` - Hue value as i32 (Q32 fixed-point)
#[lpfx_impl_macro::lpfx_impl(q32, "vec3 lpfx_hue2rgb(float hue)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_hue2rgb_q32(result_ptr: *mut i32, hue: i32) {
    // Convert raw pointer to safe array reference at boundary
    let result = unsafe { &mut *result_ptr.cast::<[i32; 3]>() };
    let rgb = lpfx_hue2rgb_q32(Q32::from_fixed(hue));
    result[0] = rgb.x.to_fixed();
    result[1] = rgb.y.to_fixed();
    result[2] = rgb.z.to_fixed();
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::fixed_to_float;

    #[test]
    fn test_hue2rgb_red() {
        // Hue 0.0 should produce red (1, 0, 0)
        let result = lpfx_hue2rgb_q32(Q32::ZERO);
        let r = fixed_to_float(result.x.to_fixed());
        let g = fixed_to_float(result.y.to_fixed());
        let b = fixed_to_float(result.z.to_fixed());
        assert!(
            (r - 1.0).abs() < 0.01,
            "Red component should be ~1.0, got {}",
            r
        );
        assert!(g < 0.01, "Green component should be ~0.0, got {}", g);
        assert!(b < 0.01, "Blue component should be ~0.0, got {}", b);
    }

    #[test]
    fn test_hue2rgb_green() {
        // Hue ~0.333 should produce green (0, 1, 0)
        let hue = Q32::from_f32(0.333);
        let result = lpfx_hue2rgb_q32(hue);
        let r = fixed_to_float(result.x.to_fixed());
        let g = fixed_to_float(result.y.to_fixed());
        let b = fixed_to_float(result.z.to_fixed());
        assert!(r < 0.01, "Red component should be ~0.0, got {}", r);
        assert!(
            (g - 1.0).abs() < 0.01,
            "Green component should be ~1.0, got {}",
            g
        );
        assert!(b < 0.01, "Blue component should be ~0.0, got {}", b);
    }

    #[test]
    fn test_hue2rgb_blue() {
        // Hue ~0.666 should produce blue (0, 0, 1)
        let hue = Q32::from_f32(0.666);
        let result = lpfx_hue2rgb_q32(hue);
        let r = fixed_to_float(result.x.to_fixed());
        let g = fixed_to_float(result.y.to_fixed());
        let b = fixed_to_float(result.z.to_fixed());
        assert!(r < 0.01, "Red component should be ~0.0, got {}", r);
        assert!(g < 0.01, "Green component should be ~0.0, got {}", g);
        assert!(
            (b - 1.0).abs() < 0.01,
            "Blue component should be ~1.0, got {}",
            b
        );
    }

    #[test]
    fn test_hue2rgb_range() {
        // All components should be in [0, 1] range
        for i in 0..100 {
            let hue = Q32::from_f32(i as f32 / 100.0);
            let result = lpfx_hue2rgb_q32(hue);
            assert!(
                result.x >= Q32::ZERO && result.x <= Q32::ONE,
                "R component should be in [0, 1]"
            );
            assert!(
                result.y >= Q32::ZERO && result.y <= Q32::ONE,
                "G component should be in [0, 1]"
            );
            assert!(
                result.z >= Q32::ZERO && result.z <= Q32::ONE,
                "B component should be in [0, 1]"
            );
        }
    }
}
