//! 2D Fractal Brownian Motion noise function.
//!
//! Combines multiple octaves of 2D noise to create fractal patterns.

use crate::builtins::lpfx::generative::snoise::snoise2_q32::lpfx_snoise2;
use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec2_q32::Vec2Q32;

/// FBM constants matching GLSL defaults
const VALUE_INITIAL: Q32 = Q32::ZERO;
const AMPLITUDE_INITIAL: Q32 = Q32::HALF; // 0.5
const SCALE_SCALAR: Q32 = Q32(131072); // 2.0 in Q16.16
const AMPLITUDE_SCALAR: Q32 = Q32::HALF; // 0.5

/// 2D Fractal Brownian Motion noise function
///
/// # Arguments
/// * `p` - Input coordinates as Vec2Q32
/// * `octaves` - Number of octaves to combine
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value as Q32
#[inline(always)]
pub fn lpfx_fbm2(p: Vec2Q32, octaves: i32, seed: u32) -> Q32 {
    // Initial values
    let mut value = VALUE_INITIAL;
    let mut amplitude = AMPLITUDE_INITIAL;
    let mut st = p;

    // Loop of octaves
    for _ in 0..octaves {
        value += amplitude * lpfx_snoise2(st, seed);
        st = st * SCALE_SCALAR;
        amplitude *= AMPLITUDE_SCALAR;
    }
    value
}

/// 2D Fractal Brownian Motion noise function (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate as i32 (Q32 fixed-point)
/// * `y` - Y coordinate as i32 (Q32 fixed-point)
/// * `octaves` - Number of octaves as i32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value as i32 (Q32 fixed-point format)
#[lpfx_impl_macro::lpfx_impl(q32, "float lpfx_fbm(vec2 p, int octaves, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_fbm2_q32(x: i32, y: i32, octaves: i32, seed: u32) -> i32 {
    lpfx_fbm2(
        Vec2Q32::new(Q32::from_fixed(x), Q32::from_fixed(y)),
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
    fn test_fbm2_basic() {
        let result = __lpfx_fbm2_q32(
            Q32::from_f32(42.5).to_fixed(),
            Q32::from_f32(10.3).to_fixed(),
            4,
            123,
        );
        // Just verify it doesn't crash and produces a value
        let _val = Q32::from_fixed(result).to_f32();
    }
}
