//! 2D Gradient Noise function.
//!
//! Uses random values at grid cell corners and interpolates between them using cubic smoothing.
//!
//! This implementation was derived from LYGIA's gnoise.glsl, which uses the Prosperity License.
//! Gradient noise (also called Value noise) is a standard algorithm documented in graphics
//! literature. The core concept (random values at grid points + interpolation) is mathematical
//! procedure, not copyrightable expression. This Rust/Q32 port is our own implementation.

use crate::builtins::lpfn::generative::gnoise::smooth_lut_q32::cubic_vec2_lut;
use crate::builtins::lpfn::generative::random::random2_q32::lpfn_random2;
use lps_q32::fns::mix_q32;
use lps_q32::q32::Q32;
use lps_q32::vec2_q32::Vec2Q32;

/// 2D Gradient Noise function
///
/// # Arguments
/// * `p` - Input coordinates as Vec2Q32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value in [0, 1] range as Q32
#[inline(always)]
pub fn lpfn_gnoise2(p: Vec2Q32, seed: u32) -> Q32 {
    // i = floor(p), f = fract(p)
    let i = p.floor();
    let f = p.fract();

    // Sample corners
    let a = lpfn_random2(i, seed);
    let b = lpfn_random2(i + Vec2Q32::new(Q32::ONE, Q32::ZERO), seed);
    let c = lpfn_random2(i + Vec2Q32::new(Q32::ZERO, Q32::ONE), seed);
    let d = lpfn_random2(i + Vec2Q32::one(), seed);

    // Interpolate using cubic smoothing (LUT-based for performance)
    let u = cubic_vec2_lut(f);

    // Bilinear interpolation with cross terms
    // mix(a, b, u.x) + (c - a) * u.y * (1.0 - u.x) + (d - b) * u.x * u.y
    let ab = mix_q32(a, b, u.x);
    let ca = c - a;
    let db = d - b;
    let one_minus_ux = Q32::ONE - u.x;
    ab + ca * u.y * one_minus_ux + db * u.x * u.y
}

/// 2D Gradient Noise function (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate as i32 (Q32 fixed-point)
/// * `y` - Y coordinate as i32 (Q32 fixed-point)
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value in [0, 1] range as i32 (Q32 fixed-point format)
#[lpfn_impl_macro::lpfn_impl(q32, "float lpfn_gnoise(vec2 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_gnoise2_q32(x: i32, y: i32, seed: u32) -> i32 {
    lpfn_gnoise2(Vec2Q32::new(Q32::from_fixed(x), Q32::from_fixed(y)), seed).to_fixed()
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    #[test]
    fn test_gnoise2_range() {
        let result = __lp_lpfn_gnoise2_q32(
            Q32::from_f32_wrapping(42.5).to_fixed(),
            Q32::from_f32_wrapping(10.3).to_fixed(),
            123,
        );
        let val = Q32::from_fixed(result).to_f32();
        assert!(val >= 0.0 && val <= 1.0, "Gnoise should be in [0, 1] range");
    }
}
