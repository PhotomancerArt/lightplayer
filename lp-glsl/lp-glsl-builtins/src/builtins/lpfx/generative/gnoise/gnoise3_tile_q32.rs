//! 3D Tilable Gradient Noise function.
//!
//! Uses srandom3_tile for seamless tiling and dot products for gradient noise.

use crate::builtins::lpfx::generative::srandom::srandom3_tile_q32::lpfx_srandom3_tile;
use crate::glsl::q32::fns::{mix_q32, quintic_vec3};
use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;

/// 3D Tilable Gradient Noise function
///
/// # Arguments
/// * `p` - Input coordinates as Vec3Q32
/// * `tile_length` - Tile length as Q32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value in [0, 1] range as Q32 (normalized)
#[inline(always)]
pub fn lpfx_gnoise3_tile(p: Vec3Q32, tile_length: Q32, seed: u32) -> Q32 {
    // i = floor(p), f = fract(p)
    let i = p.floor();
    let f = p.fract();

    // Interpolate using quintic smoothing
    let u = quintic_vec3(f);

    // Scale tile_length for srandom3_tile: tileLength * lacunarity * 0.5
    // lacunarity = 2.0, so: tileLength * 2.0 * 0.5 = tileLength
    let scaled_tile = tile_length;

    // Compute distance vectors from corners
    let f000 = f - Vec3Q32::zero();
    let f100 = f - Vec3Q32::new(Q32::ONE, Q32::ZERO, Q32::ZERO);
    let f010 = f - Vec3Q32::new(Q32::ZERO, Q32::ONE, Q32::ZERO);
    let f110 = f - Vec3Q32::new(Q32::ONE, Q32::ONE, Q32::ZERO);
    let f001 = f - Vec3Q32::new(Q32::ZERO, Q32::ZERO, Q32::ONE);
    let f101 = f - Vec3Q32::new(Q32::ONE, Q32::ZERO, Q32::ONE);
    let f011 = f - Vec3Q32::new(Q32::ZERO, Q32::ONE, Q32::ONE);
    let f111 = f - Vec3Q32::one();

    // Sample gradients at corners using srandom3_tile
    let g000 = lpfx_srandom3_tile(i + Vec3Q32::zero(), scaled_tile, seed);
    let g100 = lpfx_srandom3_tile(
        i + Vec3Q32::new(Q32::ONE, Q32::ZERO, Q32::ZERO),
        scaled_tile,
        seed,
    );
    let g010 = lpfx_srandom3_tile(
        i + Vec3Q32::new(Q32::ZERO, Q32::ONE, Q32::ZERO),
        scaled_tile,
        seed,
    );
    let g110 = lpfx_srandom3_tile(
        i + Vec3Q32::new(Q32::ONE, Q32::ONE, Q32::ZERO),
        scaled_tile,
        seed,
    );
    let g001 = lpfx_srandom3_tile(
        i + Vec3Q32::new(Q32::ZERO, Q32::ZERO, Q32::ONE),
        scaled_tile,
        seed,
    );
    let g101 = lpfx_srandom3_tile(
        i + Vec3Q32::new(Q32::ONE, Q32::ZERO, Q32::ONE),
        scaled_tile,
        seed,
    );
    let g011 = lpfx_srandom3_tile(
        i + Vec3Q32::new(Q32::ZERO, Q32::ONE, Q32::ONE),
        scaled_tile,
        seed,
    );
    let g111 = lpfx_srandom3_tile(i + Vec3Q32::one(), scaled_tile, seed);

    // Compute dot products
    let d000 = g000.dot(f000);
    let d100 = g100.dot(f100);
    let d010 = g010.dot(f010);
    let d110 = g110.dot(f110);
    let d001 = g001.dot(f001);
    let d101 = g101.dot(f101);
    let d011 = g011.dot(f011);
    let d111 = g111.dot(f111);

    // Trilinear interpolation
    let x00 = mix_q32(d000, d100, u.x);
    let x10 = mix_q32(d010, d110, u.x);
    let x01 = mix_q32(d001, d101, u.x);
    let x11 = mix_q32(d011, d111, u.x);

    let y0 = mix_q32(x00, x10, u.y);
    let y1 = mix_q32(x01, x11, u.y);

    let result = mix_q32(y0, y1, u.z);

    // Normalize to [0, 1]: result * 0.5 + 0.5
    result * Q32::HALF + Q32::HALF
}

/// 3D Tilable Gradient Noise function (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate as i32 (Q32 fixed-point)
/// * `y` - Y coordinate as i32 (Q32 fixed-point)
/// * `z` - Z coordinate as i32 (Q32 fixed-point)
/// * `tile_length` - Tile length as i32 (Q32 fixed-point)
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value in [0, 1] range as i32 (Q32 fixed-point format)
#[lpfx_impl_macro::lpfx_impl(q32, "float lpfx_gnoise(vec3 p, float tileLength, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_gnoise3_tile_q32(
    x: i32,
    y: i32,
    z: i32,
    tile_length: i32,
    seed: u32,
) -> i32 {
    lpfx_gnoise3_tile(
        Vec3Q32::new(Q32::from_fixed(x), Q32::from_fixed(y), Q32::from_fixed(z)),
        Q32::from_fixed(tile_length),
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
    fn test_gnoise3_tile_range() {
        let result = __lpfx_gnoise3_tile_q32(
            Q32::from_f32(42.5).to_fixed(),
            Q32::from_f32(10.3).to_fixed(),
            Q32::from_f32(5.7).to_fixed(),
            Q32::from_f32(10.0).to_fixed(),
            123,
        );
        let val = Q32::from_fixed(result).to_f32();
        assert!(
            val >= 0.0 && val <= 1.0,
            "Gnoise tile should be in [0, 1] range"
        );
    }
}
