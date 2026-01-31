//! Convert RGB color space to HSV (float implementation - stub).
//!
//! This is a stub implementation that will be replaced with a proper float implementation later.
//! For now, it calls the q32 version with conversion.

use crate::builtins::lpfx::color::space::rgb2hsv_q32::__lpfx_rgb2hsv_q32;
use crate::builtins::lpfx::color::space::rgb2hsv_q32::__lpfx_rgb2hsv_vec4_q32;
use crate::glsl::q32::types::q32::Q32;

/// Convert RGB color to HSV color (extern C wrapper for compiler).
///
/// Uses result pointer parameter to return vec3: writes all components to memory.
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where vec3 result will be written (result pointer parameter)
/// * `x` - R component as f32
/// * `y` - G component as f32
/// * `z` - B component as f32
#[lpfx_impl_macro::lpfx_impl(f32, "vec3 lpfx_rgb2hsv(vec3 rgb)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_rgb2hsv_f32(result_ptr: *mut f32, x: f32, y: f32, z: f32) {
    // Convert raw pointer to safe array reference at boundary
    let result = unsafe { &mut *result_ptr.cast::<[f32; 3]>() };
    // Stub: convert to q32, call q32 version, convert back
    let x_q32 = Q32::from_f32(x);
    let y_q32 = Q32::from_f32(y);
    let z_q32 = Q32::from_f32(z);
    let mut result_q32 = [0i32; 3];
    __lpfx_rgb2hsv_q32(
        result_q32.as_mut_ptr(),
        x_q32.to_fixed(),
        y_q32.to_fixed(),
        z_q32.to_fixed(),
    );
    result[0] = Q32::from_fixed(result_q32[0]).to_f32();
    result[1] = Q32::from_fixed(result_q32[1]).to_f32();
    result[2] = Q32::from_fixed(result_q32[2]).to_f32();
}

/// Convert RGB color to HSV color with alpha (extern C wrapper for compiler).
///
/// Uses result pointer parameter to return vec4: writes all components to memory.
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where vec4 result will be written (result pointer parameter)
/// * `x` - R component as f32
/// * `y` - G component as f32
/// * `z` - B component as f32
/// * `w` - A component as f32
#[lpfx_impl_macro::lpfx_impl(f32, "vec4 lpfx_rgb2hsv(vec4 rgb)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_rgb2hsv_vec4_f32(result_ptr: *mut f32, x: f32, y: f32, z: f32, w: f32) {
    // Convert raw pointer to safe array reference at boundary
    let result = unsafe { &mut *result_ptr.cast::<[f32; 4]>() };
    // Stub: convert to q32, call q32 version, convert back
    let x_q32 = Q32::from_f32(x);
    let y_q32 = Q32::from_f32(y);
    let z_q32 = Q32::from_f32(z);
    let w_q32 = Q32::from_f32(w);
    let mut result_q32 = [0i32; 4];
    __lpfx_rgb2hsv_vec4_q32(
        result_q32.as_mut_ptr(),
        x_q32.to_fixed(),
        y_q32.to_fixed(),
        z_q32.to_fixed(),
        w_q32.to_fixed(),
    );
    result[0] = Q32::from_fixed(result_q32[0]).to_f32();
    result[1] = Q32::from_fixed(result_q32[1]).to_f32();
    result[2] = Q32::from_fixed(result_q32[2]).to_f32();
    result[3] = Q32::from_fixed(result_q32[3]).to_f32();
}
