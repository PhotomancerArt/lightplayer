//! R16 UNORM texture sampling entry points (native-f32 ABI): single channel
//! expanded to vec4 like `texelFetch` — the f32 sibling of
//! [`super::r16_unorm_q32`].
//!
//! The addressing is shared with the Q32 path: wrap modes, texel byte offsets
//! and the filter/wrap ABI decode are integer work with no float in them. Only
//! the coordinate→index math ([`super::sample_ref_f32`]), the unorm→lane
//! decode and the blend differ.
//!
//! **Tolerance:** exact against the canonical sampling contract. The blend is
//! `a + t*(b - a)` in f32, which is the same formula the Q32 path approximates
//! in fixed point — so this side has no rounding budget to spend.

use lps_shared::texture_format::TextureFilter;

use super::sample_ref_f32::{LinearAxisF32, linear_indices_f32, nearest_index_f32};
use super::sampler_helpers::{
    Texture1dUnormSampleArgsF32, Texture2dUnormSampleArgsF32, decode_filter_abi, decode_wrap_abi,
    f32_lerp, load_r16_texel_lane_f32, texel_rel_byte_offset,
};

/// # Safety
/// `out` must be valid for four consecutive `f32` writes. `ptr` and following lanes must describe a
/// texture whose bytes are readable through `ptr` interpreted as a guest offset / host pointer per target.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __lp_texture2d_r16_unorm_f32(
    out: *mut f32,
    ptr: u32,
    width: u32,
    height: u32,
    row_stride: u32,
    u: f32,
    v: f32,
    filter_abi: u32,
    wrap_x_abi: u32,
    wrap_y_abi: u32,
) {
    let base = ptr as *const u8;
    let args = Texture2dUnormSampleArgsF32 {
        width,
        height,
        row_stride,
        u,
        v,
        filter_abi,
        wrap_x_abi,
        wrap_y_abi,
    };
    let lanes = unsafe { texture2d_r16_unorm_sample_f32(base, args) };
    unsafe {
        core::ptr::copy_nonoverlapping(lanes.as_ptr(), out, 4);
    }
}

/// # Safety
/// `out` must be valid for four consecutive `f32` writes. `ptr` and following lanes must describe a
/// readable height-one / 1D texture row as above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __lp_texture1d_r16_unorm_f32(
    out: *mut f32,
    ptr: u32,
    width: u32,
    row_stride: u32,
    u: f32,
    filter_abi: u32,
    wrap_x_abi: u32,
) {
    let base = ptr as *const u8;
    let args = Texture1dUnormSampleArgsF32 {
        width,
        row_stride,
        u,
        filter_abi,
        wrap_x_abi,
    };
    let lanes = unsafe { texture1d_r16_unorm_sample_f32(base, args) };
    unsafe {
        core::ptr::copy_nonoverlapping(lanes.as_ptr(), out, 4);
    }
}

/// R16 expands to `vec4(r, 0, 0, 1)` — the `texelFetch` contract, matching the
/// Q32 sibling.
#[inline]
fn vec4_fill_r16(r_lane: f32) -> [f32; 4] {
    [r_lane, 0.0, 0.0, 1.0]
}

/// Sample R16 UNORM as vec4 in 2D using packed ABI arguments.
///
/// # Safety
/// `base` must point to readable texture storage covering every texel byte addressed using `args.width`,
/// `args.height`, and `args.row_stride` under the implemented wrap/filter logic.
pub unsafe fn texture2d_r16_unorm_sample_f32(
    base: *const u8,
    args: Texture2dUnormSampleArgsF32,
) -> [f32; 4] {
    let filter = decode_filter_abi(args.filter_abi);
    let wx = decode_wrap_abi(args.wrap_x_abi);
    let wy = decode_wrap_abi(args.wrap_y_abi);

    match filter {
        TextureFilter::Nearest => {
            let ix = nearest_index_f32(args.u, args.width, wx);
            let iy = nearest_index_f32(args.v, args.height, wy);
            let r = unsafe {
                load_r16_texel_lane_f32(
                    base,
                    texel_rel_byte_offset(ix, iy, args.row_stride, R16Layout::BPP),
                )
            };
            vec4_fill_r16(r)
        }
        TextureFilter::Linear => {
            let ax = linear_indices_f32(args.u, args.width, wx);
            let ay = linear_indices_f32(args.v, args.height, wy);
            unsafe { bilinear_r16(base, args.row_stride, ax, ay) }
        }
    }
}

/// Sample R16 UNORM along X for a single row (`iy == 0`).
///
/// # Safety
/// `base` must point to readable storage for row 0 with extent `args.width` and stride `args.row_stride`.
pub unsafe fn texture1d_r16_unorm_sample_f32(
    base: *const u8,
    args: Texture1dUnormSampleArgsF32,
) -> [f32; 4] {
    let filter = decode_filter_abi(args.filter_abi);
    let wx = decode_wrap_abi(args.wrap_x_abi);
    let iy = 0u32;

    match filter {
        TextureFilter::Nearest => {
            let ix = nearest_index_f32(args.u, args.width, wx);
            let r = unsafe {
                load_r16_texel_lane_f32(
                    base,
                    texel_rel_byte_offset(ix, iy, args.row_stride, R16Layout::BPP),
                )
            };
            vec4_fill_r16(r)
        }
        TextureFilter::Linear => {
            let ax = linear_indices_f32(args.u, args.width, wx);
            unsafe { linear_rows_r16(base, args.row_stride, iy, ax) }
        }
    }
}

struct R16Layout;

impl R16Layout {
    const BPP: u32 = 2;
}

/// # Safety
/// Every index in `ax`/`ay` must address readable storage.
unsafe fn bilinear_r16(
    base: *const u8,
    row_stride: u32,
    ax: LinearAxisF32,
    ay: LinearAxisF32,
) -> [f32; 4] {
    let at = |ix: u32, iy: u32| unsafe {
        load_r16_texel_lane_f32(
            base,
            texel_rel_byte_offset(ix, iy, row_stride, R16Layout::BPP),
        )
    };
    let v00 = vec4_fill_r16(at(ax.i0, ay.i0));
    let v10 = vec4_fill_r16(at(ax.i1, ay.i0));
    let v01 = vec4_fill_r16(at(ax.i0, ay.i1));
    let v11 = vec4_fill_r16(at(ax.i1, ay.i1));

    let mut out = [0f32; 4];
    for i in 0..4 {
        let s0 = f32_lerp(v00[i], v10[i], ax.frac);
        let s1 = f32_lerp(v01[i], v11[i], ax.frac);
        out[i] = f32_lerp(s0, s1, ay.frac);
    }
    out
}

/// # Safety
/// Every index in `ax` must address readable storage on row `iy`.
unsafe fn linear_rows_r16(
    base: *const u8,
    row_stride: u32,
    iy: u32,
    ax: LinearAxisF32,
) -> [f32; 4] {
    let at = |ix: u32| unsafe {
        load_r16_texel_lane_f32(
            base,
            texel_rel_byte_offset(ix, iy, row_stride, R16Layout::BPP),
        )
    };
    let v0 = vec4_fill_r16(at(ax.i0));
    let v1 = vec4_fill_r16(at(ax.i1));
    let mut out = [0f32; 4];
    for i in 0..4 {
        out[i] = f32_lerp(v0[i], v1[i], ax.frac);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins::texture::r16_unorm_q32::texture2d_r16_unorm_sample;
    use crate::builtins::texture::sampler_helpers::Texture2dUnormSampleArgs;
    use lps_q32::Q32;
    use lps_shared::texture_format::{TextureFilter, TextureWrap};

    /// A 4x4 R16 ramp.
    fn texture() -> [u16; 16] {
        core::array::from_fn(|i| (i as u16) * 4369) // 4369 = 65535/15
    }

    fn sample_f32(tex: &[u16; 16], u: f32, v: f32, filter: TextureFilter) -> [f32; 4] {
        let args = Texture2dUnormSampleArgsF32 {
            width: 4,
            height: 4,
            row_stride: 8,
            u,
            v,
            filter_abi: filter.to_builtin_abi(),
            wrap_x_abi: TextureWrap::ClampToEdge.to_builtin_abi(),
            wrap_y_abi: TextureWrap::ClampToEdge.to_builtin_abi(),
        };
        unsafe { texture2d_r16_unorm_sample_f32(tex.as_ptr().cast(), args) }
    }

    fn sample_q32(tex: &[u16; 16], u: f32, v: f32, filter: TextureFilter) -> [f32; 4] {
        let args = Texture2dUnormSampleArgs {
            width: 4,
            height: 4,
            row_stride: 8,
            u: Q32::from_f32_wrapping(u).to_fixed(),
            v: Q32::from_f32_wrapping(v).to_fixed(),
            filter_abi: filter.to_builtin_abi(),
            wrap_x_abi: TextureWrap::ClampToEdge.to_builtin_abi(),
            wrap_y_abi: TextureWrap::ClampToEdge.to_builtin_abi(),
        };
        let lanes = unsafe { texture2d_r16_unorm_sample(tex.as_ptr().cast(), args) };
        lanes.map(|l| l as f32 / 65536.0)
    }

    #[test]
    fn r16_expands_to_the_texelfetch_contract() {
        let tex = texture();
        let v = sample_f32(&tex, 0.5, 0.5, TextureFilter::Nearest);
        assert_eq!(v[1], 0.0);
        assert_eq!(v[2], 0.0);
        assert_eq!(v[3], 1.0);
    }

    /// The two float modes must sample the same texture the same way; a
    /// disagreement here would make every textured shader mode-dependent.
    #[test]
    fn agrees_with_the_q32_sampler() {
        let tex = texture();
        for filter in [TextureFilter::Nearest, TextureFilter::Linear] {
            for i in 0..=20 {
                for j in 0..=20 {
                    let (u, v) = (i as f32 * 0.05, j as f32 * 0.05);
                    let f = sample_f32(&tex, u, v, filter);
                    let q = sample_q32(&tex, u, v, filter);
                    for c in 0..4 {
                        assert!(
                            (f[c] - q[c]).abs() < 2e-4,
                            "{filter:?} ({u},{v}) lane {c}: {} vs {}",
                            f[c],
                            q[c]
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn every_lane_stays_in_the_unit_interval() {
        let tex = texture();
        for i in -10..=30 {
            for j in -10..=30 {
                let v = sample_f32(
                    &tex,
                    i as f32 * 0.05,
                    j as f32 * 0.05,
                    TextureFilter::Linear,
                );
                for c in v {
                    assert!((0.0..=1.0).contains(&c), "lane {c}");
                }
            }
        }
    }
}
