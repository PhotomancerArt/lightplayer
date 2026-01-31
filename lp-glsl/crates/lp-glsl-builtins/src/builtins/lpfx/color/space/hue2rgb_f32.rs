//! Convert hue value to RGB color (float implementation - stub).
//!
//! This is a stub implementation that will be replaced with a proper float implementation later.
//! For now, it calls the q32 version with conversion.

use crate::builtins::lpfx::color::space::hue2rgb_q32::__lpfx_hue2rgb_q32;
use crate::glsl::q32::types::q32::Q32;

/// Convert hue value to RGB color (extern C wrapper for compiler).
///
/// Uses result pointer parameter to return vec3: writes all components to memory.
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where vec3 result will be written (result pointer parameter)
/// * `hue` - Hue value as f32
#[lpfx_impl_macro::lpfx_impl(f32, "vec3 lpfx_hue2rgb(float hue)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_hue2rgb_f32(result_ptr: *mut f32, hue: f32) {
    // Convert raw pointer to safe array reference at boundary
    let result = unsafe { &mut *result_ptr.cast::<[f32; 3]>() };
    // Stub: convert to q32, call q32 version, convert back
    let hue_q32 = Q32::from_f32(hue);
    let mut result_q32 = [0i32; 3];
    __lpfx_hue2rgb_q32(result_q32.as_mut_ptr(), hue_q32.to_fixed());
    result[0] = Q32::from_fixed(result_q32[0]).to_f32();
    result[1] = Q32::from_fixed(result_q32[1]).to_f32();
    result[2] = Q32::from_fixed(result_q32[2]).to_f32();
}
