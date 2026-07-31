//! 3D Worley noise function value variant (float implementation - stub).
//!
//! **Unimplemented.** Every function here traps via
//! [`crate::f32_unimplemented::f32_unimplemented`]. These
//! are placeholders whose signatures, `lpfn_impl` annotations and builtin-table
//! wiring are correct; only the bodies are missing. They previously round-tripped
//! through Q32 via `Q32::from_f32_wrapping`, which silently returned
//! Q32-precision results with wrapped range — the exact property native f32 is
//! being added for. Failing loudly is deliberate: see the f32 roadmap, M5.

/// 3D Worley noise function value variant (float version).
///
/// # Arguments
/// * `x` - X coordinate as f32
/// * `y` - Y coordinate as f32
/// * `z` - Z coordinate as f32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Hash value of nearest cell approximately in range [-1, 1] as f32
#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_worley_value(vec3 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_worley3_value_f32(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let _ = (x, y, z, seed);
    crate::f32_unimplemented::f32_unimplemented()
}
