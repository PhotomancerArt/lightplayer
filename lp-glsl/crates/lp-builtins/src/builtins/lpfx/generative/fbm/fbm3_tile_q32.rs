//! 3D Tilable Fractal Brownian Motion noise function.
//!
//! Combines multiple octaves of tilable 3D gradient noise to create fractal patterns.

use crate::builtins::lpfx::generative::gnoise::gnoise3_tile_q32::lpfx_gnoise3_tile;
use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;

/// FBM constants for tilable variant
const PERSISTENCE: Q32 = Q32::HALF; // 0.5
const LACUNARITY: Q32 = Q32(131072); // 2.0 in Q16.16

/// 3D Tilable Fractal Brownian Motion noise function
///
/// # Arguments
/// * `p` - Input coordinates as Vec3Q32
/// * `tile_length` - Tile length as Q32
/// * `octaves` - Number of octaves to combine
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value in [0, 1] range as Q32 (normalized)
#[inline(always)]
pub fn lpfx_fbm3_tile(p: Vec3Q32, tile_length: Q32, octaves: i32, seed: u32) -> Q32 {
    let mut amplitude = Q32::HALF; // 0.5
    let mut total = Q32::ZERO;
    let mut normalization = Q32::ZERO;
    let mut pos = p;

    for _ in 0..octaves {
        // Scale tile_length: tileLength * lacunarity * 0.5
        let scaled_tile = tile_length * LACUNARITY * Q32::HALF;
        let noise_value = lpfx_gnoise3_tile(pos, scaled_tile, seed);
        // noise_value is already in [0, 1] range from gnoise3_tile
        total += noise_value * amplitude;
        normalization += amplitude;
        amplitude *= PERSISTENCE;
        pos = pos * LACUNARITY;
    }

    total / normalization
}

/// 3D Tilable Fractal Brownian Motion noise function (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate as i32 (Q32 fixed-point)
/// * `y` - Y coordinate as i32 (Q32 fixed-point)
/// * `z` - Z coordinate as i32 (Q32 fixed-point)
/// * `tile_length` - Tile length as i32 (Q32 fixed-point)
/// * `octaves` - Number of octaves as i32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value in [0, 1] range as i32 (Q32 fixed-point format)
#[lpfx_impl_macro::lpfx_impl(
    q32,
    "float lpfx_fbm(vec3 p, float tileLength, int octaves, uint seed)"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_fbm3_tile_q32(
    x: i32,
    y: i32,
    z: i32,
    tile_length: i32,
    octaves: i32,
    seed: u32,
) -> i32 {
    lpfx_fbm3_tile(
        Vec3Q32::new(Q32::from_fixed(x), Q32::from_fixed(y), Q32::from_fixed(z)),
        Q32::from_fixed(tile_length),
        octaves,
        seed,
    )
    .to_fixed()
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    #[test]
    fn test_fbm3_tile_range() {
        let result = __lpfx_fbm3_tile_q32(
            Q32::from_f32(42.5).to_fixed(),
            Q32::from_f32(10.3).to_fixed(),
            Q32::from_f32(5.7).to_fixed(),
            Q32::from_f32(10.0).to_fixed(),
            4,
            123,
        );
        let val = Q32::from_fixed(result).to_f32();
        assert!(
            val >= 0.0 && val <= 1.0,
            "FBM tile should be in [0, 1] range"
        );
    }
}
