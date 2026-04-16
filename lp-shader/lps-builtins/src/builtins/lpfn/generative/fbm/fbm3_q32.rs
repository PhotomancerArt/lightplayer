//! 3D Fractal Brownian Motion noise function.
//!
//! Combines multiple octaves of 3D noise to create fractal patterns.

use crate::builtins::lpfn::generative::snoise::snoise3_q32::lpfn_snoise3;
use lps_q32::q32::Q32;
use lps_q32::vec3_q32::Vec3Q32;

/// FBM constants matching GLSL defaults
const VALUE_INITIAL: Q32 = Q32::ZERO;
const AMPLITUDE_INITIAL: Q32 = Q32::HALF; // 0.5
const SCALE_SCALAR: Q32 = Q32(131072); // 2.0 in Q16.16
const AMPLITUDE_SCALAR: Q32 = Q32::HALF; // 0.5

/// 3D Fractal Brownian Motion noise function
///
/// # Arguments
/// * `p` - Input coordinates as Vec3Q32
/// * `octaves` - Number of octaves to combine
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value as Q32
#[inline(always)]
pub fn lpfn_fbm3(p: Vec3Q32, octaves: i32, seed: u32) -> Q32 {
    // Initial values
    let mut value = VALUE_INITIAL;
    let mut amplitude = AMPLITUDE_INITIAL;
    let mut pos = p;

    // Loop of octaves
    for _ in 0..octaves {
        value += amplitude * lpfn_snoise3(pos, seed);
        pos = pos * SCALE_SCALAR;
        amplitude *= AMPLITUDE_SCALAR;
    }
    value
}

/// 3D Fractal Brownian Motion noise function (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate as i32 (Q32 fixed-point)
/// * `y` - Y coordinate as i32 (Q32 fixed-point)
/// * `z` - Z coordinate as i32 (Q32 fixed-point)
/// * `octaves` - Number of octaves as i32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value as i32 (Q32 fixed-point format)
#[lpfn_impl_macro::lpfn_impl(q32, "float lpfn_fbm(vec3 p, int octaves, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_fbm3_q32(x: i32, y: i32, z: i32, octaves: i32, seed: u32) -> i32 {
    lpfn_fbm3(
        Vec3Q32::new(Q32::from_fixed(x), Q32::from_fixed(y), Q32::from_fixed(z)),
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
    fn test_fbm3_basic() {
        let result = __lp_lpfn_fbm3_q32(
            Q32::from_f32_wrapping(42.5).to_fixed(),
            Q32::from_f32_wrapping(10.3).to_fixed(),
            Q32::from_f32_wrapping(5.7).to_fixed(),
            4,
            123,
        );
        // Just verify it doesn't crash and produces a value
        let _val = Q32::from_fixed(result).to_f32();
    }
}
