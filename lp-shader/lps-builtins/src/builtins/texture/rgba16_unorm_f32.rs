//! RGBA16 UNORM texture sampling entry points (native-f32 ABI) — the f32
//! sibling of [`super::rgba16_unorm_q32`].
//!
//! See [`super::r16_unorm_f32`] for what is shared with the Q32 path and what
//! is not.
//!
//! **Tolerance:** exact against the canonical sampling contract.

use lps_shared::texture_format::TextureFilter;

use super::sample_ref_f32::{LinearAxisF32, linear_indices_f32, nearest_index_f32};
use super::sampler_helpers::{
    Texture1dUnormSampleArgsF32, Texture2dUnormSampleArgsF32, decode_filter_abi, decode_wrap_abi,
    f32_lerp, load_rgba16_texel_f32, texel_rel_byte_offset,
};

/// 2D normalized sampling for RGBA16 textures. Writes vec4/f32 lanes through `out`.
///
/// # Safety
/// `out` must be valid for four consecutive `f32` writes. `ptr` and descriptor lanes must describe a
/// readable 2D RGBA16 texture.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __lp_texture2d_rgba16_unorm_f32(
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
    let lanes = unsafe { texture2d_rgba16_unorm_sample_f32(base, args) };
    unsafe {
        core::ptr::copy_nonoverlapping(lanes.as_ptr(), out, 4);
    }
}

/// 1D sampling for height-one RGBA16 textures. Writes vec4/f32 lanes through `out`.
///
/// # Safety
/// Same as [`__lp_texture2d_rgba16_unorm_f32`], for a single-row height-one layout.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __lp_texture1d_rgba16_unorm_f32(
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
    let lanes = unsafe { texture1d_rgba16_unorm_sample_f32(base, args) };
    unsafe {
        core::ptr::copy_nonoverlapping(lanes.as_ptr(), out, 4);
    }
}

/// Core 2D sampler (`base` points at texel (0,0)).
///
/// # Safety
/// `base` must point to readable RGBA16 texel storage covering all addressing implied by `args`.
pub unsafe fn texture2d_rgba16_unorm_sample_f32(
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
            unsafe {
                load_rgba16_texel_f32(
                    base,
                    texel_rel_byte_offset(ix, iy, args.row_stride, Rgba16UnormLayout::BPP),
                )
            }
        }
        TextureFilter::Linear => {
            let ax = linear_indices_f32(args.u, args.width, wx);
            let ay = linear_indices_f32(args.v, args.height, wy);
            unsafe { bilinear_rgba16(base, args.row_stride, ax, ay) }
        }
    }
}

/// Height-one strip: sample row `iy == 0` only; ignores normalized `v` (no `wrap_y`).
///
/// # Safety
/// `base` must point to readable RGBA16 storage for row 0 with extent `args.width`.
pub unsafe fn texture1d_rgba16_unorm_sample_f32(
    base: *const u8,
    args: Texture1dUnormSampleArgsF32,
) -> [f32; 4] {
    let filter = decode_filter_abi(args.filter_abi);
    let wx = decode_wrap_abi(args.wrap_x_abi);
    let iy = 0u32;

    match filter {
        TextureFilter::Nearest => {
            let ix = nearest_index_f32(args.u, args.width, wx);
            unsafe {
                load_rgba16_texel_f32(
                    base,
                    texel_rel_byte_offset(ix, iy, args.row_stride, Rgba16UnormLayout::BPP),
                )
            }
        }
        TextureFilter::Linear => {
            let ax = linear_indices_f32(args.u, args.width, wx);
            unsafe { linear_rows_rgba16(base, args.row_stride, iy, ax) }
        }
    }
}

struct Rgba16UnormLayout;

impl Rgba16UnormLayout {
    const BPP: u32 = 8;
}

/// # Safety
/// Every index in `ax`/`ay` must address readable storage.
unsafe fn bilinear_rgba16(
    base: *const u8,
    row_stride: u32,
    ax: LinearAxisF32,
    ay: LinearAxisF32,
) -> [f32; 4] {
    let at = |ix: u32, iy: u32| unsafe {
        load_rgba16_texel_f32(
            base,
            texel_rel_byte_offset(ix, iy, row_stride, Rgba16UnormLayout::BPP),
        )
    };
    let c00 = at(ax.i0, ay.i0);
    let c10 = at(ax.i1, ay.i0);
    let c01 = at(ax.i0, ay.i1);
    let c11 = at(ax.i1, ay.i1);

    let mut out = [0f32; 4];
    for i in 0..4 {
        let r0 = f32_lerp(c00[i], c10[i], ax.frac);
        let r1 = f32_lerp(c01[i], c11[i], ax.frac);
        out[i] = f32_lerp(r0, r1, ay.frac);
    }
    out
}

/// # Safety
/// Every index in `ax` must address readable storage on row `iy`.
unsafe fn linear_rows_rgba16(
    base: *const u8,
    row_stride: u32,
    iy: u32,
    ax: LinearAxisF32,
) -> [f32; 4] {
    let at = |ix: u32| unsafe {
        load_rgba16_texel_f32(
            base,
            texel_rel_byte_offset(ix, iy, row_stride, Rgba16UnormLayout::BPP),
        )
    };
    let c0 = at(ax.i0);
    let c1 = at(ax.i1);
    let mut out = [0f32; 4];
    for i in 0..4 {
        out[i] = f32_lerp(c0[i], c1[i], ax.frac);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins::texture::rgba16_unorm_q32::texture2d_rgba16_unorm_sample;
    use crate::builtins::texture::sampler_helpers::Texture2dUnormSampleArgs;
    use lps_q32::Q32;
    use lps_shared::texture_format::{TextureFilter, TextureWrap};

    /// A 2x2 RGBA16 texture with distinguishable channels.
    fn texture() -> [u16; 16] {
        [
            0, 21845, 43690, 65535, // (0,0)
            65535, 43690, 21845, 0, // (1,0)
            13107, 26214, 39321, 52428, // (0,1)
            52428, 39321, 26214, 13107, // (1,1)
        ]
    }

    fn sample_f32(tex: &[u16; 16], u: f32, v: f32, filter: TextureFilter) -> [f32; 4] {
        let args = Texture2dUnormSampleArgsF32 {
            width: 2,
            height: 2,
            row_stride: 16,
            u,
            v,
            filter_abi: filter.to_builtin_abi(),
            wrap_x_abi: TextureWrap::ClampToEdge.to_builtin_abi(),
            wrap_y_abi: TextureWrap::ClampToEdge.to_builtin_abi(),
        };
        unsafe { texture2d_rgba16_unorm_sample_f32(tex.as_ptr().cast(), args) }
    }

    fn sample_q32(tex: &[u16; 16], u: f32, v: f32, filter: TextureFilter) -> [f32; 4] {
        let args = Texture2dUnormSampleArgs {
            width: 2,
            height: 2,
            row_stride: 16,
            u: Q32::from_f32_wrapping(u).to_fixed(),
            v: Q32::from_f32_wrapping(v).to_fixed(),
            filter_abi: filter.to_builtin_abi(),
            wrap_x_abi: TextureWrap::ClampToEdge.to_builtin_abi(),
            wrap_y_abi: TextureWrap::ClampToEdge.to_builtin_abi(),
        };
        let lanes = unsafe { texture2d_rgba16_unorm_sample(tex.as_ptr().cast(), args) };
        lanes.map(|l| l as f32 / 65536.0)
    }

    #[test]
    fn nearest_reads_the_addressed_texel() {
        let tex = texture();
        let v = sample_f32(&tex, 0.25, 0.25, TextureFilter::Nearest);
        // Texel (0,0) = 0, 1/3, 2/3, 1 in unorm16 terms.
        assert!((v[0] - 0.0).abs() < 1e-4, "{v:?}");
        assert!((v[3] - 65535.0 / 65536.0).abs() < 1e-4, "{v:?}");
    }

    /// Both float modes must sample identically.
    #[test]
    fn agrees_with_the_q32_sampler() {
        let tex = texture();
        for filter in [TextureFilter::Nearest, TextureFilter::Linear] {
            for i in 0..=16 {
                for j in 0..=16 {
                    let (u, v) = (i as f32 / 16.0, j as f32 / 16.0);
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
}
