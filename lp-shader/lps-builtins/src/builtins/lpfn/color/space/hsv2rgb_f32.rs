//! Convert HSV color space to RGB (float implementation - stub).
//!
//! **Unimplemented.** Every function here traps via
//! [`crate::f32_unimplemented::f32_unimplemented`]. These
//! are placeholders whose signatures, `lpfn_impl` annotations and builtin-table
//! wiring are correct; only the bodies are missing. They previously round-tripped
//! through Q32 via `Q32::from_f32_wrapping`, which silently returned
//! Q32-precision results with wrapped range — the exact property native f32 is
//! being added for. Failing loudly is deliberate: see the f32 roadmap, M5.

/// Convert HSV color to RGB color (extern C wrapper for compiler).
///
/// Uses result pointer parameter to return vec3: writes all components to memory.
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where vec3 result will be written (result pointer parameter)
/// * `x` - H component as f32
/// * `y` - S component as f32
/// * `z` - V component as f32
#[lpfn_impl_macro::lpfn_impl(f32, "vec3 lpfn_hsv2rgb(vec3 hsv)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_hsv2rgb_f32(result_ptr: *mut f32, x: f32, y: f32, z: f32) {
    let _ = (result_ptr, x, y, z);
    crate::f32_unimplemented::f32_unimplemented()
}

/// Convert HSV color to RGB color with alpha (extern C wrapper for compiler).
///
/// Uses result pointer parameter to return vec4: writes all components to memory.
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where vec4 result will be written (result pointer parameter)
/// * `x` - H component as f32
/// * `y` - S component as f32
/// * `z` - V component as f32
/// * `w` - A component as f32
#[lpfn_impl_macro::lpfn_impl(f32, "vec4 lpfn_hsv2rgb(vec4 hsv)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_hsv2rgb_vec4_f32(result_ptr: *mut f32, x: f32, y: f32, z: f32, w: f32) {
    let _ = (result_ptr, x, y, z, w);
    crate::f32_unimplemented::f32_unimplemented()
}
