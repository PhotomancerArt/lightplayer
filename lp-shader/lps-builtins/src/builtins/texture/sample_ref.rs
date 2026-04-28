//! Pure Q32 reference for normalized `texture()` sampling math (texel-center convention).
//!
//! Continuous texel coordinate: `coord = uv * extent - 0.5` (uv in Q16.16, extent ∈ `u32`).
//! Nearest: [`crate::builtins::glsl::round_q32`] on `coord`, then integer → wrapped index.
//! Linear: `floor(coord)` and `floor(coord) + 1` with fractional weight; each index wrapped.

use lps_q32::Q32;
use lps_shared::texture_format::TextureWrap;

/// Neighbor indices and Q16.16 weight toward `i1` (weight toward `i0` is `1 - frac`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinearAxis {
    pub i0: u32,
    pub i1: u32,
    /// Interpolation weight for `i1` in Q16.16; `0..=65535` for in-range fractions.
    pub frac: i32,
}

/// Texel-space coordinate before wrap: `uv * extent - 0.5` (Q16.16).
#[inline]
pub fn texel_center_coord_q32(uv: i32, extent: u32) -> i32 {
    let prod = (uv as i64).wrapping_mul(extent as i64);
    let wide = prod.wrapping_sub(i64::from(Q32::HALF.to_fixed()));
    sat_i64_to_q32_raw(wide)
}

/// Map an integer texel index through wrap to `[0, extent - 1]`.
#[inline]
pub fn wrap_coord(coord: i32, extent: u32, wrap: TextureWrap) -> u32 {
    if extent == 0 {
        return 0;
    }
    match wrap {
        TextureWrap::ClampToEdge => {
            let max = (extent - 1) as i32;
            coord.clamp(0, max) as u32
        }
        TextureWrap::Repeat => coord.rem_euclid(extent as i32) as u32,
        TextureWrap::MirrorRepeat => mirror_repeat_index(coord, extent),
    }
}

#[inline]
pub fn nearest_index_q32(uv: i32, extent: u32, wrap: TextureWrap) -> u32 {
    let coord = texel_center_coord_q32(uv, extent);
    let idx = round_coord_to_nearest_i32(coord);
    wrap_coord(idx, extent, wrap)
}

#[inline]
pub fn linear_indices_q32(uv: i32, extent: u32, wrap: TextureWrap) -> LinearAxis {
    let coord = texel_center_coord_q32(uv, extent);
    let q = Q32::from_fixed(coord);
    let i0 = q.to_i32();
    let floor_q = Q32::from_i32(i0);
    let frac_q = q - floor_q;
    let i1 = i0.wrapping_add(1);
    LinearAxis {
        i0: wrap_coord(i0, extent, wrap),
        i1: wrap_coord(i1, extent, wrap),
        frac: frac_q.to_fixed(),
    }
}

/// Height-one / 1D path: nearest index from `u` only; `v` is ignored.
#[inline]
pub fn nearest_index_height_one_q32(u: i32, v: i32, width: u32, wrap_x: TextureWrap) -> u32 {
    let _ = v;
    nearest_index_q32(u, width, wrap_x)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use lps_shared::texture_format::TextureWrap;

    fn uv(f: f32) -> i32 {
        Q32::from_f32_wrapping(f).to_fixed()
    }

    #[test]
    fn texture_sample_ref_center_uv_half_4_wide() {
        let u = uv(0.5);
        let c = texel_center_coord_q32(u, 4);
        assert_eq!(c, Q32::from_f32_wrapping(1.5).to_fixed());
        assert_eq!(
            nearest_index_q32(u, 4, TextureWrap::ClampToEdge),
            2,
            "u=0.5, w=4 → coord 1.5 → nearest 2"
        );
    }

    #[test]
    fn texture_sample_ref_edges_clamp() {
        let w = 8u32;
        // Just inside 0: expect texel 0
        let u0 = uv(1.0 / (w as f32 * 64.0));
        assert_eq!(nearest_index_q32(u0, w, TextureWrap::ClampToEdge), 0);
        // Just inside 1 from the left: last texel
        let u1 = uv(1.0 - 1.0 / (w as f32 * 64.0));
        assert_eq!(nearest_index_q32(u1, w, TextureWrap::ClampToEdge), w - 1);
    }

    #[test]
    fn texture_sample_ref_repeat_negative_and_over_one() {
        let w = 4u32;
        // coord = -0.25 * 4 - 0.5 = -1.5 → round −2 → (−2).rem_euclid(4) = 2
        let u_neg = uv(-0.25);
        assert_eq!(nearest_index_q32(u_neg, w, TextureWrap::Repeat), 2);

        let u_over = uv(1.25);
        assert_eq!(nearest_index_q32(u_over, w, TextureWrap::Repeat), 1);
    }

    #[test]
    fn texture_sample_ref_mirror_repeat_periods() {
        let w = 4u32;
        // period 6: ... 0 1 2 3 2 1 ...
        assert_eq!(wrap_coord(0, w, TextureWrap::MirrorRepeat), 0);
        assert_eq!(wrap_coord(3, w, TextureWrap::MirrorRepeat), 3);
        assert_eq!(wrap_coord(4, w, TextureWrap::MirrorRepeat), 2);
        assert_eq!(wrap_coord(5, w, TextureWrap::MirrorRepeat), 1);
        assert_eq!(wrap_coord(6, w, TextureWrap::MirrorRepeat), 0);
        assert_eq!(wrap_coord(-1, w, TextureWrap::MirrorRepeat), 1);
    }

    #[test]
    fn texture_sample_ref_linear_indices_and_frac() {
        let w = 4u32;
        let u = uv(0.5);
        let ax = linear_indices_q32(u, w, TextureWrap::ClampToEdge);
        assert_eq!(ax.i0, 1);
        assert_eq!(ax.i1, 2);
        assert_eq!(ax.frac, Q32::HALF.to_fixed(), "coord 1.5 → frac 0.5");
    }

    #[test]
    fn texture_sample_ref_height_one_ignores_v() {
        let w = 6u32;
        let u = uv(0.5);
        let v0 = uv(0.0);
        let v1 = uv(0.73);
        assert_eq!(
            nearest_index_height_one_q32(u, v0, w, TextureWrap::Repeat),
            nearest_index_height_one_q32(u, v1, w, TextureWrap::Repeat)
        );
    }

    #[test]
    fn texture_sample_ref_linear_repeat_distinct_neighbors() {
        let w = 4u32;
        let u = uv(0.875);
        let ax = linear_indices_q32(u, w, TextureWrap::Repeat);
        // coord = 0.875 * 4 - 0.5 = 3.0 → floor 3, i1 wraps to 0
        assert_eq!(ax.i0, 3);
        assert_eq!(ax.i1, 0);
        assert_eq!(ax.frac, 0);
    }
}

#[inline]
fn mirror_repeat_index(i: i32, extent: u32) -> u32 {
    if extent <= 1 {
        return 0;
    }
    let n = extent as i32;
    let period = 2 * (n - 1);
    let x = i.rem_euclid(period);
    let x = if x >= n { 2 * (n - 1) - x } else { x };
    x as u32
}

#[inline]
fn round_coord_to_nearest_i32(coord_raw: i32) -> i32 {
    crate::builtins::glsl::round_q32::__lps_round_q32(coord_raw) >> 16
}

#[inline]
fn sat_i64_to_q32_raw(wide: i64) -> i32 {
    const Q32_MAX_RAW: i64 = 0x7FFF_FFFF;
    if wide > Q32_MAX_RAW {
        Q32_MAX_RAW as i32
    } else if wide < i32::MIN as i64 {
        i32::MIN
    } else {
        wide as i32
    }
}
