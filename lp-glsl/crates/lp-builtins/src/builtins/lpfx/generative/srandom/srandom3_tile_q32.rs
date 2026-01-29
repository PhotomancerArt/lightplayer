//! 3D Signed Random function with tiling.
//!
//! Returns vec3 in [-1, 1] range after applying mod(p, tileLength)

use crate::builtins::lpfx::generative::srandom::srandom3_vec_q32::lpfx_srandom3_vec;
use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;

/// 3D Signed Random function with tiling
///
/// # Arguments
/// * `p` - Input coordinates as Vec3Q32
/// * `tile_length` - Tile length as Q32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Random vec3 in [-1, 1] range as Vec3Q32
#[inline(always)]
pub fn lpfx_srandom3_tile(p: Vec3Q32, tile_length: Q32, seed: u32) -> Vec3Q32 {
    // mod(p, tile_length) component-wise
    let p_mod = Vec3Q32::new(
        Q32::from_fixed(crate::builtins::q32::__lp_q32_mod(
            p.x.to_fixed(),
            tile_length.to_fixed(),
        )),
        Q32::from_fixed(crate::builtins::q32::__lp_q32_mod(
            p.y.to_fixed(),
            tile_length.to_fixed(),
        )),
        Q32::from_fixed(crate::builtins::q32::__lp_q32_mod(
            p.z.to_fixed(),
            tile_length.to_fixed(),
        )),
    );

    // Call srandom3_vec on modded coordinates
    lpfx_srandom3_vec(p_mod, seed)
}

/// 3D Signed Random function with tiling (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate as i32 (Q32 fixed-point)
/// * `y` - Y coordinate as i32 (Q32 fixed-point)
/// * `z` - Z coordinate as i32 (Q32 fixed-point)
/// * `tile_length` - Tile length as i32 (Q32 fixed-point)
/// * `seed` - Seed value for randomization
/// * `out` - Pointer to output vec3 [x, y, z] as i32
#[lpfx_impl_macro::lpfx_impl(q32, "vec3 lpfx_srandom3_tile(vec3 p, float tileLength, uint seed)")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_srandom3_tile_q32(
    out: *mut i32,
    x: i32,
    y: i32,
    z: i32,
    tile_length: i32,
    seed: u32,
) {
    let result = lpfx_srandom3_tile(
        Vec3Q32::new(Q32::from_fixed(x), Q32::from_fixed(y), Q32::from_fixed(z)),
        Q32::from_fixed(tile_length),
        seed,
    );
    unsafe {
        *out = result.x.to_fixed();
        *out.add(1) = result.y.to_fixed();
        *out.add(2) = result.z.to_fixed();
    }
}
