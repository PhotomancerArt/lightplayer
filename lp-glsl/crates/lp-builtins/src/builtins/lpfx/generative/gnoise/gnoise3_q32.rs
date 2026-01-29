//! 3D Gradient Noise function.
//!
//! Uses random values at grid cell corners and interpolates between them using quintic smoothing.

use crate::builtins::lpfx::generative::random::random3_q32::lpfx_random3;
use crate::glsl::q32::fns::{mix_q32, quintic_vec3};
use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;

/// 3D Gradient Noise function
///
/// # Arguments
/// * `p` - Input coordinates as Vec3Q32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value in [-1, 1] range as Q32
#[inline(always)]
pub fn lpfx_gnoise3(p: Vec3Q32, seed: u32) -> Q32 {
    // i = floor(p), f = fract(p)
    let i = p.floor();
    let f = p.fract();

    // Interpolate using quintic smoothing
    let u = quintic_vec3(f);

    // Sample all 8 corners
    let c000 = lpfx_random3(i + Vec3Q32::zero(), seed);
    let c100 = lpfx_random3(i + Vec3Q32::new(Q32::ONE, Q32::ZERO, Q32::ZERO), seed);
    let c010 = lpfx_random3(i + Vec3Q32::new(Q32::ZERO, Q32::ONE, Q32::ZERO), seed);
    let c110 = lpfx_random3(i + Vec3Q32::new(Q32::ONE, Q32::ONE, Q32::ZERO), seed);
    let c001 = lpfx_random3(i + Vec3Q32::new(Q32::ZERO, Q32::ZERO, Q32::ONE), seed);
    let c101 = lpfx_random3(i + Vec3Q32::new(Q32::ONE, Q32::ZERO, Q32::ONE), seed);
    let c011 = lpfx_random3(i + Vec3Q32::new(Q32::ZERO, Q32::ONE, Q32::ONE), seed);
    let c111 = lpfx_random3(i + Vec3Q32::one(), seed);

    // Trilinear interpolation: mix(mix(mix(...), mix(...), u.y), mix(mix(...), mix(...), u.y), u.z)
    let x00 = mix_q32(c000, c100, u.x);
    let x10 = mix_q32(c010, c110, u.x);
    let x01 = mix_q32(c001, c101, u.x);
    let x11 = mix_q32(c011, c111, u.x);

    let y0 = mix_q32(x00, x10, u.y);
    let y1 = mix_q32(x01, x11, u.y);

    let result = mix_q32(y0, y1, u.z);

    // Convert from [0, 1] to [-1, 1]
    Q32::from_f32(-1.0) + Q32::from_f32(2.0) * result
}

/// 3D Gradient Noise function (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate as i32 (Q32 fixed-point)
/// * `y` - Y coordinate as i32 (Q32 fixed-point)
/// * `z` - Z coordinate as i32 (Q32 fixed-point)
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value in [-1, 1] range as i32 (Q32 fixed-point format)
#[lpfx_impl_macro::lpfx_impl(q32, "float lpfx_gnoise(vec3 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_gnoise3_q32(x: i32, y: i32, z: i32, seed: u32) -> i32 {
    lpfx_gnoise3(
        Vec3Q32::new(Q32::from_fixed(x), Q32::from_fixed(y), Q32::from_fixed(z)),
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
    fn test_gnoise3_range() {
        let result = __lpfx_gnoise3_q32(
            Q32::from_f32(42.5).to_fixed(),
            Q32::from_f32(10.3).to_fixed(),
            Q32::from_f32(5.7).to_fixed(),
            123,
        );
        let val = Q32::from_fixed(result).to_f32();
        assert!(
            val >= -1.0 && val <= 1.0,
            "Gnoise should be in [-1, 1] range"
        );
    }
}
