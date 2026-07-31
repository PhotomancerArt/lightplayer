//! Convert RGB color space to HSV (float implementation - stub).
//!
//! **Unimplemented.** Every function here traps via
//! [`crate::f32_unimplemented::f32_unimplemented`]. These
//! are placeholders whose signatures, `lpfn_impl` annotations and builtin-table
//! wiring are correct; only the bodies are missing. They previously round-tripped
//! through Q32 via `Q32::from_f32_wrapping`, which silently returned
//! Q32-precision results with wrapped range — the exact property native f32 is
//! being added for. Failing loudly is deliberate: see the f32 roadmap, M5.

/// Convert RGB color to HSV color (extern C wrapper for compiler).
///
/// Uses result pointer parameter to return vec3: writes all components to memory.
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where vec3 result will be written (result pointer parameter)
/// * `x` - R component as f32
/// * `y` - G component as f32
/// * `z` - B component as f32
#[lpfn_impl_macro::lpfn_impl(f32, "vec3 lpfn_rgb2hsv(vec3 rgb)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_rgb2hsv_f32(result_ptr: *mut f32, x: f32, y: f32, z: f32) {
    let _ = (result_ptr, x, y, z);
    crate::f32_unimplemented::f32_unimplemented()
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
#[lpfn_impl_macro::lpfn_impl(f32, "vec4 lpfn_rgb2hsv(vec4 rgb)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_rgb2hsv_vec4_f32(result_ptr: *mut f32, x: f32, y: f32, z: f32, w: f32) {
    let _ = (result_ptr, x, y, z, w);
    crate::f32_unimplemented::f32_unimplemented()
}
