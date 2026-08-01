//! Tiling 3D signed random vec3 (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/generative/srandom/srandom3_tile.glsl` (normative): wrap the
//! lattice coordinate with `mod(p, tileLength)`, then evaluate
//! `lpfn_srandom3_vec` on the wrapped coordinate.
//!
//! The wrap is GLSL `mod`, **not** the truncated remainder: negative lattice
//! coordinates must wrap into `[0, tileLength)` or the tile seam shows.
//!
//! **Tolerance:** chaotic sin-hash — statistical, not pointwise.

use super::srandom3_vec_f32::srandom3_vec;
use crate::f32_math::glsl_mod;

/// Tiling 3D signed random vec3 (float version).
///
/// # Arguments
/// * `out` - Pointer to memory where the vec3 result is written
/// * `x` / `y` / `z` - Input coordinate as f32
/// * `tile_length` - Tile period
/// * `seed` - Accepted for signature compatibility; unused downstream
#[lpfn_impl_macro::lpfn_impl(f32, "vec3 lpfn_srandom3_tile(vec3 p, float tileLength, uint seed)")]
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes to caller-provided out-pointer"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_srandom3_tile_f32(
    out: *mut f32,
    x: f32,
    y: f32,
    z: f32,
    tile_length: f32,
    seed: u32,
) {
    let _ = seed;
    let v = srandom3_tile(x, y, z, tile_length);
    unsafe {
        *out = v[0];
        *out.add(1) = v[1];
        *out.add(2) = v[2];
    }
}

/// Rust-facing form; the tiling gradient noise calls this per cell corner.
#[inline]
pub(crate) fn srandom3_tile(x: f32, y: f32, z: f32, tile_length: f32) -> [f32; 3] {
    srandom3_vec(
        glsl_mod(x, tile_length),
        glsl_mod(y, tile_length),
        glsl_mod(z, tile_length),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_actually_tiles() {
        let t = 8.0f32;
        for i in 0..5 {
            let p = i as f32;
            assert_eq!(
                srandom3_tile(p, p, p, t),
                srandom3_tile(p + t, p + t, p + t, t),
                "period {t} at {p}"
            );
        }
    }

    #[test]
    fn negative_coordinates_wrap_into_the_tile() {
        // GLSL `mod`, not `%`: -1 must land on tileLength - 1, or the seam
        // is visible at the origin.
        let t = 8.0f32;
        assert_eq!(
            srandom3_tile(-1.0, -1.0, -1.0, t),
            srandom3_tile(7.0, 7.0, 7.0, t)
        );
    }

    #[test]
    fn writes_all_three_out_lanes() {
        let mut out = [f32::NAN; 3];
        __lp_lpfn_srandom3_tile_f32(out.as_mut_ptr(), 1.0, 2.0, 3.0, 8.0, 0);
        assert_eq!(out, srandom3_tile(1.0, 2.0, 3.0, 8.0));
    }
}
