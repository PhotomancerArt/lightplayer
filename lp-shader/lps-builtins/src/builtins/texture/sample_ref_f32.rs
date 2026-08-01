//! Native-f32 reference for normalized `texture()` sampling math — the f32
//! sibling of [`super::sample_ref`], same texel-center convention.
//!
//! Continuous texel coordinate: `coord = uv * extent - 0.5`.
//! Nearest: `round(coord)` (ties away from zero, matching
//! [`crate::builtins::glsl::round_f32`] and the Q32 sibling), then integer →
//! wrapped index.
//! Linear: `floor(coord)` and `floor(coord) + 1` with fractional weight; each
//! index wrapped.
//!
//! **The wrap arithmetic is shared, not duplicated.** [`super::sample_ref::wrap_coord`]
//! is pure integer index math — clamp, `rem_euclid`, mirror period — with no
//! float representation in it at all, so both modes call the same function.
//! Only the three genuinely float-shaped steps (scale-and-bias, round, split
//! into index + fraction) have an f32 form here.

use lps_shared::texture_format::TextureWrap;

use super::sample_ref::wrap_coord;

/// Neighbor indices and f32 weight toward `i1` (weight toward `i0` is `1 - frac`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearAxisF32 {
    pub i0: u32,
    pub i1: u32,
    /// Interpolation weight for `i1`; `0.0..=1.0` for in-range fractions.
    pub frac: f32,
}

/// Texel-space coordinate before wrap: `uv * extent - 0.5`.
///
/// No saturation, unlike the Q32 sibling: `uv * extent` cannot overflow f32 for
/// any texture size we can address, so clamping here would only hide a bug.
#[inline]
pub fn texel_center_coord_f32(uv: f32, extent: u32) -> f32 {
    uv * extent as f32 - 0.5
}

#[inline]
pub fn nearest_index_f32(uv: f32, extent: u32, wrap: TextureWrap) -> u32 {
    let coord = texel_center_coord_f32(uv, extent);
    // Ties away from zero, matching `round_q32` and `libm::roundf` — the same
    // choice `round_f32` documents.
    let idx = libm::roundf(coord) as i32;
    wrap_coord(idx, extent, wrap)
}

#[inline]
pub fn linear_indices_f32(uv: f32, extent: u32, wrap: TextureWrap) -> LinearAxisF32 {
    let coord = texel_center_coord_f32(uv, extent);
    let floor = crate::f32_math::floor(coord);
    let i0 = floor as i32;
    LinearAxisF32 {
        i0: wrap_coord(i0, extent, wrap),
        i1: wrap_coord(i0.wrapping_add(1), extent, wrap),
        frac: coord - floor,
    }
}

/// Height-one / 1D path: nearest index from `u` only; `v` is ignored.
#[inline]
pub fn nearest_index_height_one_f32(u: f32, v: f32, width: u32, wrap_x: TextureWrap) -> u32 {
    let _ = v;
    nearest_index_f32(u, width, wrap_x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins::texture::sample_ref::{linear_indices_q32, nearest_index_q32};
    use lps_q32::Q32;

    /// The f32 and Q32 index math must agree wherever Q16.16 can represent the
    /// coordinate — a sampler that picked different texels per float mode would
    /// make every textured shader mode-dependent.
    #[test]
    fn agrees_with_the_q32_index_math_across_the_representable_range() {
        for extent in [1u32, 2, 4, 7, 8, 64, 256] {
            for wrap in [
                TextureWrap::ClampToEdge,
                TextureWrap::Repeat,
                TextureWrap::MirrorRepeat,
            ] {
                for i in -300..=300 {
                    let uv = i as f32 * 0.01;
                    let q = nearest_index_q32(Q32::from_f32_wrapping(uv).to_fixed(), extent, wrap);
                    let f = nearest_index_f32(uv, extent, wrap);
                    assert_eq!(f, q, "nearest uv={uv} extent={extent} wrap={wrap:?}");
                }
            }
        }
    }

    #[test]
    fn linear_neighbors_agree_with_the_q32_index_math() {
        for extent in [2u32, 4, 8, 64] {
            for wrap in [TextureWrap::ClampToEdge, TextureWrap::Repeat] {
                for i in -200..=200 {
                    let uv = i as f32 * 0.01;
                    let q = linear_indices_q32(Q32::from_f32_wrapping(uv).to_fixed(), extent, wrap);
                    let f = linear_indices_f32(uv, extent, wrap);
                    assert_eq!(f.i0, q.i0, "i0 uv={uv} extent={extent}");
                    assert_eq!(f.i1, q.i1, "i1 uv={uv} extent={extent}");
                    // The Q32 side quantizes `uv` to 1/65536 before scaling by
                    // `extent`, so its fraction can only agree to about
                    // `extent / 65536`. That is a property of the Q32 input
                    // encoding, not of this index math.
                    let q_frac = q.frac as f32 / 65536.0;
                    let band = 2.0 * extent as f32 / 65536.0;
                    assert!(
                        (f.frac - q_frac).abs() <= band,
                        "frac uv={uv} extent={extent}: {} vs {q_frac} (band {band})",
                        f.frac
                    );
                }
            }
        }
    }

    #[test]
    fn center_uv_half_of_a_four_wide_texture() {
        // coord = 0.5 * 4 - 0.5 = 1.5 → nearest 2 (ties away from zero).
        assert_eq!(texel_center_coord_f32(0.5, 4), 1.5);
        assert_eq!(nearest_index_f32(0.5, 4, TextureWrap::ClampToEdge), 2);
    }

    #[test]
    fn coordinates_far_outside_q32_range_still_wrap() {
        // The capability: Q16.16 saturates near 32768, so a large uv used to
        // clamp to the same texel forever. f32 keeps wrapping.
        let a = nearest_index_f32(1.0e6, 4, TextureWrap::Repeat);
        let b = nearest_index_f32(1.0e6 + 0.25, 4, TextureWrap::Repeat);
        assert!(a < 4 && b < 4);
    }

    #[test]
    fn height_one_ignores_v() {
        assert_eq!(
            nearest_index_height_one_f32(0.5, 0.0, 6, TextureWrap::Repeat),
            nearest_index_height_one_f32(0.5, 0.73, 6, TextureWrap::Repeat)
        );
    }
}
