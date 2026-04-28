//! R16 UNORM texture sampling entry points (Q32 ABI): single channel expanded to vec4 like `texelFetch`.

use lps_q32::Q32;
use lps_shared::texture_format::TextureFilter;

use super::sample_ref::{LinearAxis, linear_indices_q32, nearest_index_q32};
use super::sampler_helpers::{
    Texture1dUnormSampleArgs, Texture2dUnormSampleArgs, decode_filter_abi, decode_wrap_abi,
    load_r16_texel_lane, q32_lerp, texel_rel_byte_offset,
};

/// # Safety
/// `out` must be valid for four consecutive `i32` writes. `ptr` and following lanes must describe a
/// texture whose bytes are readable through `ptr` interpreted as a guest offset / host pointer per target.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __lp_texture2d_r16_unorm_q32(
    out: *mut i32,
    ptr: u32,
    width: u32,
    height: u32,
    row_stride: u32,
    u: i32,
    v: i32,
    filter_abi: u32,
    wrap_x_abi: u32,
    wrap_y_abi: u32,
) {
    let base = ptr as *const u8;
    let args = Texture2dUnormSampleArgs {
        width,
        height,
        row_stride,
        u,
        v,
        filter_abi,
        wrap_x_abi,
        wrap_y_abi,
    };
    let lanes = unsafe { texture2d_r16_unorm_sample(base, args) };
    unsafe {
        core::ptr::copy_nonoverlapping(lanes.as_ptr(), out, 4);
    }
}

/// # Safety
/// `out` must be valid for four consecutive `i32` writes. `ptr` and following lanes must describe a
/// readable height-one / 1D texture row as above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __lp_texture1d_r16_unorm_q32(
    out: *mut i32,
    ptr: u32,
    width: u32,
    row_stride: u32,
    u: i32,
    filter_abi: u32,
    wrap_x_abi: u32,
) {
    let base = ptr as *const u8;
    let args = Texture1dUnormSampleArgs {
        width,
        row_stride,
        u,
        filter_abi,
        wrap_x_abi,
    };
    let lanes = unsafe { texture1d_r16_unorm_sample(base, args) };
    unsafe {
        core::ptr::copy_nonoverlapping(lanes.as_ptr(), out, 4);
    }
}

#[inline]
fn vec4_fill_r16(r_lane: i32) -> [i32; 4] {
    [
        r_lane,
        Q32::ZERO.to_fixed(),
        Q32::ZERO.to_fixed(),
        Q32::ONE.to_fixed(),
    ]
}

/// Sample R16 UNORM as vec4 in 2D using packed ABI arguments.
///
/// # Safety
/// `base` must point to readable texture storage covering every texel byte addressed using `args.width`,
/// `args.height`, and `args.row_stride` under the implemented wrap/filter logic.
pub unsafe fn texture2d_r16_unorm_sample(
    base: *const u8,
    args: Texture2dUnormSampleArgs,
) -> [i32; 4] {
    let filter = decode_filter_abi(args.filter_abi);
    let wx = decode_wrap_abi(args.wrap_x_abi);
    let wy = decode_wrap_abi(args.wrap_y_abi);

    match filter {
        TextureFilter::Nearest => {
            let ix = nearest_index_q32(args.u, args.width, wx);
            let iy = nearest_index_q32(args.v, args.height, wy);
            let r = unsafe {
                load_r16_texel_lane(
                    base,
                    texel_rel_byte_offset(ix, iy, args.row_stride, R16Layout::BPP),
                )
            };
            vec4_fill_r16(r)
        }
        TextureFilter::Linear => {
            let ax = linear_indices_q32(args.u, args.width, wx);
            let ay = linear_indices_q32(args.v, args.height, wy);
            bilinear_r16(base, args.row_stride, ax, ay)
        }
    }
}

/// Sample R16 UNORM along X for a single row (`iy == 0`).
///
/// # Safety
/// `base` must point to readable storage for row 0 with extent `args.width` and stride `args.row_stride`.
pub unsafe fn texture1d_r16_unorm_sample(
    base: *const u8,
    args: Texture1dUnormSampleArgs,
) -> [i32; 4] {
    let filter = decode_filter_abi(args.filter_abi);
    let wx = decode_wrap_abi(args.wrap_x_abi);
    let iy = 0u32;

    match filter {
        TextureFilter::Nearest => {
            let ix = nearest_index_q32(args.u, args.width, wx);
            let r = unsafe {
                load_r16_texel_lane(
                    base,
                    texel_rel_byte_offset(ix, iy, args.row_stride, R16Layout::BPP),
                )
            };
            vec4_fill_r16(r)
        }
        TextureFilter::Linear => {
            let ax = linear_indices_q32(args.u, args.width, wx);
            linear_rows_r16(base, args.row_stride, iy, ax)
        }
    }
}

struct R16Layout;

impl R16Layout {
    const BPP: u32 = 2;
}

fn bilinear_r16(base: *const u8, row_stride: u32, ax: LinearAxis, ay: LinearAxis) -> [i32; 4] {
    let r00 = unsafe {
        load_r16_texel_lane(
            base,
            texel_rel_byte_offset(ax.i0, ay.i0, row_stride, R16Layout::BPP),
        )
    };
    let r10 = unsafe {
        load_r16_texel_lane(
            base,
            texel_rel_byte_offset(ax.i1, ay.i0, row_stride, R16Layout::BPP),
        )
    };
    let r01 = unsafe {
        load_r16_texel_lane(
            base,
            texel_rel_byte_offset(ax.i0, ay.i1, row_stride, R16Layout::BPP),
        )
    };
    let r11 = unsafe {
        load_r16_texel_lane(
            base,
            texel_rel_byte_offset(ax.i1, ay.i1, row_stride, R16Layout::BPP),
        )
    };

    let v00 = vec4_fill_r16(r00);
    let v10 = vec4_fill_r16(r10);
    let v01 = vec4_fill_r16(r01);
    let v11 = vec4_fill_r16(r11);

    let mut out = [0i32; 4];
    for i in 0..4 {
        let s0 = q32_lerp(v00[i], v10[i], ax.frac);
        let s1 = q32_lerp(v01[i], v11[i], ax.frac);
        out[i] = q32_lerp(s0, s1, ay.frac);
    }
    out
}

fn linear_rows_r16(base: *const u8, row_stride: u32, iy: u32, ax: LinearAxis) -> [i32; 4] {
    let r0 = unsafe {
        load_r16_texel_lane(
            base,
            texel_rel_byte_offset(ax.i0, iy, row_stride, R16Layout::BPP),
        )
    };
    let r1 = unsafe {
        load_r16_texel_lane(
            base,
            texel_rel_byte_offset(ax.i1, iy, row_stride, R16Layout::BPP),
        )
    };
    let v0 = vec4_fill_r16(r0);
    let v1 = vec4_fill_r16(r1);
    let mut out = [0i32; 4];
    for i in 0..4 {
        out[i] = q32_lerp(v0[i], v1[i], ax.frac);
    }
    out
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use lps_q32::Q32;

    use super::*;
    use crate::builtins::lpir::unorm_conv_q32::__lp_lpir_unorm16_to_f_q32;
    use crate::builtins::texture::sample_ref::{
        linear_indices_q32 as ref_linear, nearest_index_q32 as ref_nearest,
    };
    use crate::builtins::texture::sampler_helpers::{load_r16_texel_lane, texel_rel_byte_offset};
    use lps_shared::texture_format::{TextureFilter, TextureWrap};

    fn uv(f: f32) -> i32 {
        Q32::from_f32_wrapping(f).to_fixed()
    }

    #[test]
    fn texture_r16_vec4_fill_matches_texelfetch_contract() {
        let raw = 12345u16;
        let r_lane = __lp_lpir_unorm16_to_f_q32(raw as i32);
        let v = vec4_fill_r16(r_lane);
        assert_eq!(v[0], r_lane);
        assert_eq!(v[1], 0);
        assert_eq!(v[2], 0);
        assert_eq!(v[3], Q32::ONE.to_fixed());

        let mut buf = alloc::vec![0u8; 4];
        buf[..2].copy_from_slice(&raw.to_le_bytes());
        let loaded = unsafe { load_r16_texel_lane(buf.as_ptr(), 0) };
        assert_eq!(vec4_fill_r16(loaded), v);
    }

    #[test]
    fn texture_r16_builtin_matches_sample_ref_nearest_2d() {
        let w = 4u32;
        let h = 3u32;
        let rs = w * 2;
        let mut buf = alloc::vec![0xffu8; (rs * h) as usize];
        let mut k = 1000u16;
        for iy in 0..h {
            for ix in 0..w {
                let off = texel_rel_byte_offset(ix, iy, rs, 2);
                buf[off..off + 2].copy_from_slice(&k.to_le_bytes());
                k = k.wrapping_add(300);
            }
        }
        let u = uv(0.62);
        let v = uv(0.33);
        let wx = TextureWrap::ClampToEdge;
        let wy = TextureWrap::Repeat;
        let got = unsafe {
            // `buf` is contiguous RGBA16-style storage for this synthetic R16 grid.
            texture2d_r16_unorm_sample(
                buf.as_ptr(),
                Texture2dUnormSampleArgs {
                    width: w,
                    height: h,
                    row_stride: rs,
                    u,
                    v,
                    filter_abi: TextureFilter::Nearest.to_builtin_abi(),
                    wrap_x_abi: wx.to_builtin_abi(),
                    wrap_y_abi: wy.to_builtin_abi(),
                },
            )
        };
        let ix = ref_nearest(u, w, wx);
        let iy = ref_nearest(v, h, wy);
        let r = unsafe { load_r16_texel_lane(buf.as_ptr(), texel_rel_byte_offset(ix, iy, rs, 2)) };
        assert_eq!(got, vec4_fill_r16(r));
    }

    #[test]
    fn texture_r16_builtin_matches_reference_linear_2d() {
        let w = 3u32;
        let h = 4u32;
        let rs = w * 2;
        let mut buf = alloc::vec![0u8; (rs * h) as usize];
        let mut k = 500u16;
        for iy in 0..h {
            for ix in 0..w {
                let off = texel_rel_byte_offset(ix, iy, rs, 2);
                buf[off..off + 2].copy_from_slice(&k.to_le_bytes());
                k = k.wrapping_add(700);
            }
        }
        let u = uv(0.41);
        let v = uv(0.77);
        let wx = TextureWrap::MirrorRepeat;
        let wy = TextureWrap::ClampToEdge;
        let ax = ref_linear(u, w, wx);
        let ay = ref_linear(v, h, wy);
        let r00 = unsafe {
            load_r16_texel_lane(buf.as_ptr(), texel_rel_byte_offset(ax.i0, ay.i0, rs, 2))
        };
        let r10 = unsafe {
            load_r16_texel_lane(buf.as_ptr(), texel_rel_byte_offset(ax.i1, ay.i0, rs, 2))
        };
        let r01 = unsafe {
            load_r16_texel_lane(buf.as_ptr(), texel_rel_byte_offset(ax.i0, ay.i1, rs, 2))
        };
        let r11 = unsafe {
            load_r16_texel_lane(buf.as_ptr(), texel_rel_byte_offset(ax.i1, ay.i1, rs, 2))
        };
        let mut exp = [0i32; 4];
        let v00 = vec4_fill_r16(r00);
        let v10 = vec4_fill_r16(r10);
        let v01 = vec4_fill_r16(r01);
        let v11 = vec4_fill_r16(r11);
        for i in 0..4 {
            let s0 = q32_lerp(v00[i], v10[i], ax.frac);
            let s1 = q32_lerp(v01[i], v11[i], ax.frac);
            exp[i] = q32_lerp(s0, s1, ay.frac);
        }
        let got = unsafe {
            texture2d_r16_unorm_sample(
                buf.as_ptr(),
                Texture2dUnormSampleArgs {
                    width: w,
                    height: h,
                    row_stride: rs,
                    u,
                    v,
                    filter_abi: TextureFilter::Linear.to_builtin_abi(),
                    wrap_x_abi: wx.to_builtin_abi(),
                    wrap_y_abi: wy.to_builtin_abi(),
                },
            )
        };
        assert_eq!(got, exp);
    }
}
