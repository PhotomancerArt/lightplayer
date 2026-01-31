//! 3D Tilable Fractal Brownian Motion noise function (float implementation - stub).

use crate::builtins::lpfx::generative::fbm::fbm3_tile_q32::__lpfx_fbm3_tile_q32;
use crate::glsl::q32::types::q32::Q32;

#[lpfx_impl_macro::lpfx_impl(
    f32,
    "float lpfx_fbm(vec3 p, float tileLength, int octaves, uint seed)"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_fbm3_tile_f32(
    x: f32,
    y: f32,
    z: f32,
    tile_length: f32,
    octaves: i32,
    seed: u32,
) -> f32 {
    let x_q32 = Q32::from_f32(x);
    let y_q32 = Q32::from_f32(y);
    let z_q32 = Q32::from_f32(z);
    let tile_length_q32 = Q32::from_f32(tile_length);
    let result_fixed = __lpfx_fbm3_tile_q32(
        x_q32.to_fixed(),
        y_q32.to_fixed(),
        z_q32.to_fixed(),
        tile_length_q32.to_fixed(),
        octaves,
        seed,
    );
    Q32::from_fixed(result_fixed).to_f32()
}
