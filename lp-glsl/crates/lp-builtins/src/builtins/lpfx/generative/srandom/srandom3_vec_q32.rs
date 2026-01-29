//! 3D Signed Random function returning Vec3Q32.
//!
//! Returns vec3 in [-1, 1] range using dot products with different constant vectors

use crate::builtins::q32::__lp_q32_sin;
use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;

/// Random constant multiplier
/// In Q16.16: 43758.5453123 * 65536 ≈ 2867801088
const RANDOM_MULT: i64 = 2867801088;

/// Dot product constants for each component
/// vec3(127.1, 311.7, 74.7) for x component
const DOT_X_X: i32 = 8331264; // 127.1 * 65536
const DOT_X_Y: i32 = 20422656; // 311.7 * 65536
const DOT_X_Z: i32 = 4896768; // 74.7 * 65536

/// vec3(269.5, 183.3, 246.1) for y component
const DOT_Y_X: i32 = 17661952; // 269.5 * 65536
const DOT_Y_Y: i32 = 12017664; // 183.3 * 65536
const DOT_Y_Z: i32 = 16130048; // 246.1 * 65536

/// vec3(113.5, 271.9, 124.6) for z component
const DOT_Z_X: i32 = 7438336; // 113.5 * 65536
const DOT_Z_Y: i32 = 17825792; // 271.9 * 65536
const DOT_Z_Z: i32 = 8167936; // 124.6 * 65536

/// 3D Signed Random function returning Vec3Q32
///
/// # Arguments
/// * `p` - Input coordinates as Vec3Q32
/// * `seed` - Seed value for randomization (unused in this implementation)
///
/// # Returns
/// Random vec3 in [-1, 1] range as Vec3Q32
#[inline(always)]
pub fn lpfx_srandom3_vec(p: Vec3Q32, _seed: u32) -> Vec3Q32 {
    // Compute dot products for each component
    let dot_x = ((p.x.to_fixed() as i64 * DOT_X_X as i64) >> 16) as i32
        + ((p.y.to_fixed() as i64 * DOT_X_Y as i64) >> 16) as i32
        + ((p.z.to_fixed() as i64 * DOT_X_Z as i64) >> 16) as i32;

    let dot_y = ((p.x.to_fixed() as i64 * DOT_Y_X as i64) >> 16) as i32
        + ((p.y.to_fixed() as i64 * DOT_Y_Y as i64) >> 16) as i32
        + ((p.z.to_fixed() as i64 * DOT_Y_Z as i64) >> 16) as i32;

    let dot_z = ((p.x.to_fixed() as i64 * DOT_Z_X as i64) >> 16) as i32
        + ((p.y.to_fixed() as i64 * DOT_Z_Y as i64) >> 16) as i32
        + ((p.z.to_fixed() as i64 * DOT_Z_Z as i64) >> 16) as i32;

    // sin(dot) * 43758.5453123, then fract, then -1.0 + 2.0 * fract
    let sin_x = __lp_q32_sin(dot_x);
    let multiplied_x = ((sin_x as i64 * RANDOM_MULT) >> 16) as i32;
    let fract_x = Q32::from_fixed(multiplied_x).frac();
    let result_x = Q32::from_f32(-1.0) + Q32::from_f32(2.0) * fract_x;

    let sin_y = __lp_q32_sin(dot_y);
    let multiplied_y = ((sin_y as i64 * RANDOM_MULT) >> 16) as i32;
    let fract_y = Q32::from_fixed(multiplied_y).frac();
    let result_y = Q32::from_f32(-1.0) + Q32::from_f32(2.0) * fract_y;

    let sin_z = __lp_q32_sin(dot_z);
    let multiplied_z = ((sin_z as i64 * RANDOM_MULT) >> 16) as i32;
    let fract_z = Q32::from_fixed(multiplied_z).frac();
    let result_z = Q32::from_f32(-1.0) + Q32::from_f32(2.0) * fract_z;

    Vec3Q32::new(result_x, result_y, result_z)
}

/// 3D Signed Random function returning Vec3Q32 (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate as i32 (Q32 fixed-point)
/// * `y` - Y coordinate as i32 (Q32 fixed-point)
/// * `z` - Z coordinate as i32 (Q32 fixed-point)
/// * `seed` - Seed value for randomization
/// * `out` - Pointer to output vec3 [x, y, z] as i32
#[lpfx_impl_macro::lpfx_impl(q32, "vec3 lpfx_srandom3_vec(vec3 p, uint seed)")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_srandom3_vec_q32(out: *mut i32, x: i32, y: i32, z: i32, seed: u32) {
    let result = lpfx_srandom3_vec(
        Vec3Q32::new(Q32::from_fixed(x), Q32::from_fixed(y), Q32::from_fixed(z)),
        seed,
    );
    unsafe {
        *out = result.x.to_fixed();
        *out.add(1) = result.y.to_fixed();
        *out.add(2) = result.z.to_fixed();
    }
}
