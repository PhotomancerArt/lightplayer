//! 3D Periodic Simplex Rotational Domain noise function (float implementation - stub).
//!
//! **Unimplemented.** Every function here traps via
//! [`crate::f32_unimplemented::f32_unimplemented`]. These
//! are placeholders whose signatures, `lpfn_impl` annotations and builtin-table
//! wiring are correct; only the bodies are missing. They previously round-tripped
//! through Q32 via `Q32::from_f32_wrapping`, which silently returned
//! Q32-precision results with wrapped range — the exact property native f32 is
//! being added for. Failing loudly is deliberate: see the f32 roadmap, M5.
//!
//! # Source
//!
//! This is a derivative work based on the psrdnoise implementation from Lygia:
//! https://github.com/patriciogonzalezvivo/lygia/blob/main/generative/psrdnoise.glsl
//!
//! Original algorithm by Stefan Gustavson and Ian McEwan:
//! https://github.com/stegu/psrdnoise
//!
//! # License
//!
//! Original work:
//! Copyright 2021-2023 by Stefan Gustavson and Ian McEwan.
//! Published under the terms of the MIT license:
//! https://opensource.org/license/mit/
//!
//! This derivative work (Rust/f32 wrapper implementation):
//! Also published under the terms of the MIT license.

/// 3D Periodic Simplex Rotational Domain noise function (float version).
///
/// # Arguments
/// * `x` - X coordinate as f32
/// * `y` - Y coordinate as f32
/// * `z` - Z coordinate as f32
/// * `period_x` - X period as f32 (0 = no tiling)
/// * `period_y` - Y period as f32 (0 = no tiling)
/// * `period_z` - Z period as f32 (0 = no tiling)
/// * `alpha` - Rotation angle in radians as f32
/// * `gradient_out` - Pointer to output gradient [gx, gy, gz] as f32
/// * `seed` - Seed value for randomization (unused in psrdnoise, kept for consistency)
///
/// # Returns
/// Noise value approximately in range [-1, 1] as f32
#[lpfn_impl_macro::lpfn_impl(
    f32,
    "float lpfn_psrdnoise(vec3 x, vec3 period, float alpha, out vec3 gradient, uint seed)"
)]
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes gradient through caller-provided out-pointer"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_psrdnoise3_f32(
    x: f32,
    y: f32,
    z: f32,
    period_x: f32,
    period_y: f32,
    period_z: f32,
    alpha: f32,
    gradient_out: *mut f32,
    seed: u32,
) -> f32 {
    let _ = (
        x,
        y,
        z,
        period_x,
        period_y,
        period_z,
        alpha,
        gradient_out,
        seed,
    );
    crate::f32_unimplemented::f32_unimplemented()
}
