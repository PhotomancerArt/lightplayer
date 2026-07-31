//! 3D Signed Random function returning Vec3Q32 (float implementation - stub).
//!
//! **Unimplemented.** Every function here traps via
//! [`crate::f32_unimplemented::f32_unimplemented`]. These
//! are placeholders whose signatures, `lpfn_impl` annotations and builtin-table
//! wiring are correct; only the bodies are missing. They previously round-tripped
//! through Q32 via `Q32::from_f32_wrapping`, which silently returned
//! Q32-precision results with wrapped range — the exact property native f32 is
//! being added for. Failing loudly is deliberate: see the f32 roadmap, M5.

#[lpfn_impl_macro::lpfn_impl(f32, "vec3 lpfn_srandom3_vec(vec3 p, uint seed)")]
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes vec3 through caller-provided out-pointer"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_srandom3_vec_f32(out: *mut f32, x: f32, y: f32, z: f32, seed: u32) {
    let _ = (out, x, y, z, seed);
    crate::f32_unimplemented::f32_unimplemented()
}
