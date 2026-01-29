//! 3D Signed Random function with tiling (float implementation - stub).

use crate::builtins::lpfx::generative::srandom::srandom3_tile_q32::__lpfx_srandom3_tile_q32;
use crate::glsl::q32::types::q32::Q32;

#[lpfx_impl_macro::lpfx_impl(f32, "vec3 lpfx_srandom3_tile(vec3 p, float tileLength, uint seed)")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_srandom3_tile_f32(
    out: *mut f32,
    x: f32,
    y: f32,
    z: f32,
    tile_length: f32,
    seed: u32,
) {
    let x_q32 = Q32::from_f32(x);
    let y_q32 = Q32::from_f32(y);
    let z_q32 = Q32::from_f32(z);
    let tile_length_q32 = Q32::from_f32(tile_length);

    let mut result_q32 = [0i32; 3];
    __lpfx_srandom3_tile_q32(
        result_q32.as_mut_ptr(),
        x_q32.to_fixed(),
        y_q32.to_fixed(),
        z_q32.to_fixed(),
        tile_length_q32.to_fixed(),
        seed,
    );

    unsafe {
        *out = Q32::from_fixed(result_q32[0]).to_f32();
        *out.add(1) = Q32::from_fixed(result_q32[1]).to_f32();
        *out.add(2) = Q32::from_fixed(result_q32[2]).to_f32();
    }
}
