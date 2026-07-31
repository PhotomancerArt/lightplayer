//! 1D Signed Random function (float implementation - stub).
//!
//! **Unimplemented.** Every function here traps via
//! [`crate::f32_unimplemented::f32_unimplemented`]. These
//! are placeholders whose signatures, `lpfn_impl` annotations and builtin-table
//! wiring are correct; only the bodies are missing. They previously round-tripped
//! through Q32 via `Q32::from_f32_wrapping`, which silently returned
//! Q32-precision results with wrapped range — the exact property native f32 is
//! being added for. Failing loudly is deliberate: see the f32 roadmap, M5.

#[lpfn_impl_macro::lpfn_impl(f32, "float lpfn_srandom(float x, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_srandom1_f32(x: f32, seed: u32) -> f32 {
    let _ = (x, seed);
    crate::f32_unimplemented::f32_unimplemented()
}
