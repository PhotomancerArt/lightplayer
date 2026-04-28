//! RGBA16 UNORM texture sampling entry points (Q32 ABI).

use lps_shared::texture_format::TextureFilter;

use super::sample_ref::{LinearAxis, linear_indices_q32, nearest_index_q32};
use super::sampler_helpers::{
    Texture1dUnormSampleArgs, Texture2dUnormSampleArgs, decode_filter_abi, decode_wrap_abi,
    load_rgba16_texel, q32_lerp, texel_rel_byte_offset,
};

/// 2D normalized sampling for RGBA16 textures. Writes vec4/Q32 lanes through `out`.
///
/// # Safety
/// `out` must be valid for four consecutive `i32` writes. `ptr` and descriptor lanes must describe a
/// readable 2D RGBA16 texture.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __lp_texture2d_rgba16_unorm_q32(
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
    let lanes = unsafe { texture2d_rgba16_unorm_sample(base, args) };
    unsafe {
        core::ptr::copy_nonoverlapping(lanes.as_ptr(), out, 4);
    }
}

/// 1D sampling for height-one RGBA16 textures. Writes vec4/Q32 lanes through `out`.
///
/// # Safety
/// Same as [`__lp_texture2d_rgba16_unorm_q32`], for a single-row height-one layout.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __lp_texture1d_rgba16_unorm_q32(
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
    let lanes = unsafe { texture1d_rgba16_unorm_sample(base, args) };
    unsafe {
        core::ptr::copy_nonoverlapping(lanes.as_ptr(), out, 4);
    }
}

/// Core 2D sampler (`base` points at texel (0,0); used by Wasmtime dispatch with a host linear-memory base).
///
/// # Safety
/// `base` must point to readable RGBA16 texel storage covering all addressing implied by `args`.
pub unsafe fn texture2d_rgba16_unorm_sample(
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
            unsafe {
                load_rgba16_texel(
                    base,
                    texel_rel_byte_offset(ix, iy, args.row_stride, Rgba16UnormLayout::BPP),
                )
            }
        }
        TextureFilter::Linear => {
            let ax = linear_indices_q32(args.u, args.width, wx);
            let ay = linear_indices_q32(args.v, args.height, wy);
            bilinear_rgba16(base, args.row_stride, ax, ay)
        }
    }
}

/// Height-one strip: sample row `iy == 0` only; ignores normalized `v` (no `wrap_y`).
///
/// # Safety
/// `base` must point to readable RGBA16 storage for row 0 with extent `args.width`.
pub unsafe fn texture1d_rgba16_unorm_sample(
    base: *const u8,
    args: Texture1dUnormSampleArgs,
) -> [i32; 4] {
    let filter = decode_filter_abi(args.filter_abi);
    let wx = decode_wrap_abi(args.wrap_x_abi);
    let iy = 0u32;

    match filter {
        TextureFilter::Nearest => {
            let ix = nearest_index_q32(args.u, args.width, wx);
            unsafe {
                load_rgba16_texel(
                    base,
                    texel_rel_byte_offset(ix, iy, args.row_stride, Rgba16UnormLayout::BPP),
                )
            }
        }
        TextureFilter::Linear => {
            let ax = linear_indices_q32(args.u, args.width, wx);
            linear_rows_rgba16(base, args.row_stride, iy, ax)
        }
    }
}

struct Rgba16UnormLayout;

impl Rgba16UnormLayout {
    const BPP: u32 = 8;
}

fn bilinear_rgba16(base: *const u8, row_stride: u32, ax: LinearAxis, ay: LinearAxis) -> [i32; 4] {
    let c00 = unsafe {
        load_rgba16_texel(
            base,
            texel_rel_byte_offset(ax.i0, ay.i0, row_stride, Rgba16UnormLayout::BPP),
        )
    };
    let c10 = unsafe {
        load_rgba16_texel(
            base,
            texel_rel_byte_offset(ax.i1, ay.i0, row_stride, Rgba16UnormLayout::BPP),
        )
    };
    let c01 = unsafe {
        load_rgba16_texel(
            base,
            texel_rel_byte_offset(ax.i0, ay.i1, row_stride, Rgba16UnormLayout::BPP),
        )
    };
    let c11 = unsafe {
        load_rgba16_texel(
            base,
            texel_rel_byte_offset(ax.i1, ay.i1, row_stride, Rgba16UnormLayout::BPP),
        )
    };

    let mut out = [0i32; 4];
    for i in 0..4 {
        let r0 = q32_lerp(c00[i], c10[i], ax.frac);
        let r1 = q32_lerp(c01[i], c11[i], ax.frac);
        out[i] = q32_lerp(r0, r1, ay.frac);
    }
    out
}

fn linear_rows_rgba16(base: *const u8, row_stride: u32, iy: u32, ax: LinearAxis) -> [i32; 4] {
    let c0 = unsafe {
        load_rgba16_texel(
            base,
            texel_rel_byte_offset(ax.i0, iy, row_stride, Rgba16UnormLayout::BPP),
        )
    };
    let c1 = unsafe {
        load_rgba16_texel(
            base,
            texel_rel_byte_offset(ax.i1, iy, row_stride, Rgba16UnormLayout::BPP),
        )
    };
    let mut out = [0i32; 4];
    for i in 0..4 {
        out[i] = q32_lerp(c0[i], c1[i], ax.frac);
    }
    out
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use lps_q32::Q32;

    use super::*;
    use crate::builtins::texture::sample_ref::{
        linear_indices_q32 as ref_linear, nearest_index_q32 as ref_nearest,
    };
    use crate::builtins::texture::sampler_helpers::load_rgba16_texel;
    use lps_shared::texture_format::{TextureFilter, TextureWrap};

    fn uv(f: f32) -> i32 {
        Q32::from_f32_wrapping(f).to_fixed()
    }

    fn encode_rgba_texel(r: u16, g: u16, b: u16, a: u16) -> [u8; 8] {
        let mut bts = [0u8; 8];
        bts[0..2].copy_from_slice(&r.to_le_bytes());
        bts[2..4].copy_from_slice(&g.to_le_bytes());
        bts[4..6].copy_from_slice(&b.to_le_bytes());
        bts[6..8].copy_from_slice(&a.to_le_bytes());
        bts
    }

    fn ref_vec4_nearest_2d(
        buf: &[u8],
        width: u32,
        height: u32,
        row_stride: u32,
        u: i32,
        v: i32,
        wx: TextureWrap,
        wy: TextureWrap,
    ) -> [i32; 4] {
        let ix = ref_nearest(u, width, wx);
        let iy = ref_nearest(v, height, wy);
        let off = texel_rel_byte_offset(ix, iy, row_stride, 8);
        unsafe { load_rgba16_texel(buf.as_ptr(), off) }
    }

    fn ref_vec4_linear_2d(
        buf: &[u8],
        width: u32,
        height: u32,
        row_stride: u32,
        u: i32,
        v: i32,
        wx: TextureWrap,
        wy: TextureWrap,
    ) -> [i32; 4] {
        let ax = ref_linear(u, width, wx);
        let ay = ref_linear(v, height, wy);
        let c00 = unsafe {
            load_rgba16_texel(
                buf.as_ptr(),
                texel_rel_byte_offset(ax.i0, ay.i0, row_stride, 8),
            )
        };
        let c10 = unsafe {
            load_rgba16_texel(
                buf.as_ptr(),
                texel_rel_byte_offset(ax.i1, ay.i0, row_stride, 8),
            )
        };
        let c01 = unsafe {
            load_rgba16_texel(
                buf.as_ptr(),
                texel_rel_byte_offset(ax.i0, ay.i1, row_stride, 8),
            )
        };
        let c11 = unsafe {
            load_rgba16_texel(
                buf.as_ptr(),
                texel_rel_byte_offset(ax.i1, ay.i1, row_stride, 8),
            )
        };
        let mut out = [0i32; 4];
        for i in 0..4 {
            let r0 = q32_lerp(c00[i], c10[i], ax.frac);
            let r1 = q32_lerp(c01[i], c11[i], ax.frac);
            out[i] = q32_lerp(r0, r1, ay.frac);
        }
        out
    }

    #[test]
    fn texture_rgba16_builtin_matches_sample_ref_nearest_2d() {
        let w = 3u32;
        let h = 2u32;
        let rs = w * 8;
        let mut buf = alloc::vec![0u8; (rs * h) as usize];
        let t00 = encode_rgba_texel(1000, 2000, 3000, 4000);
        buf[0..8].copy_from_slice(&t00);
        let t10 = encode_rgba_texel(5000, 6000, 7000, 8000);
        buf[8..16].copy_from_slice(&t10);
        let u = uv(0.25);
        let v = uv(0.5);
        let got = unsafe {
            texture2d_rgba16_unorm_sample(
                buf.as_ptr(),
                Texture2dUnormSampleArgs {
                    width: w,
                    height: h,
                    row_stride: rs,
                    u,
                    v,
                    filter_abi: TextureFilter::Nearest.to_builtin_abi(),
                    wrap_x_abi: TextureWrap::ClampToEdge.to_builtin_abi(),
                    wrap_y_abi: TextureWrap::ClampToEdge.to_builtin_abi(),
                },
            )
        };
        let exp = ref_vec4_nearest_2d(
            &buf,
            w,
            h,
            rs,
            u,
            v,
            TextureWrap::ClampToEdge,
            TextureWrap::ClampToEdge,
        );
        assert_eq!(got, exp);
    }

    #[test]
    fn texture_rgba16_builtin_matches_sample_ref_linear_2d() {
        let w = 4u32;
        let h = 3u32;
        let rs = w * 8;
        let mut buf = alloc::vec![0xffu8; (rs * h) as usize];
        for iy in 0..h {
            for ix in 0..w {
                let px = encode_rgba_texel((ix * 1000) as u16, (iy * 500) as u16, 100, 65535);
                let off = texel_rel_byte_offset(ix, iy, rs, 8);
                buf[off..off + 8].copy_from_slice(&px);
            }
        }
        let u = uv(0.41);
        let v = uv(0.62);
        let got = unsafe {
            texture2d_rgba16_unorm_sample(
                buf.as_ptr(),
                Texture2dUnormSampleArgs {
                    width: w,
                    height: h,
                    row_stride: rs,
                    u,
                    v,
                    filter_abi: TextureFilter::Linear.to_builtin_abi(),
                    wrap_x_abi: TextureWrap::Repeat.to_builtin_abi(),
                    wrap_y_abi: TextureWrap::ClampToEdge.to_builtin_abi(),
                },
            )
        };
        let exp = ref_vec4_linear_2d(
            &buf,
            w,
            h,
            rs,
            u,
            v,
            TextureWrap::Repeat,
            TextureWrap::ClampToEdge,
        );
        assert_eq!(got, exp);
    }

    #[test]
    fn texture_rgba16_height_one_1d_matches_sample_ref_nearest() {
        let w = 5u32;
        let rs = w * 8;
        let mut buf = alloc::vec![0u8; rs as usize];
        for ix in 0..w {
            let px = encode_rgba_texel((ix * 4000) as u16, 100, 200, 300);
            let off = texel_rel_byte_offset(ix, 0, rs, 8);
            buf[off..off + 8].copy_from_slice(&px);
        }
        let u = uv(0.71);
        let wrap_x = TextureWrap::MirrorRepeat;
        let abi_f = TextureFilter::Nearest.to_builtin_abi();
        let abi_wx = wrap_x.to_builtin_abi();
        let args = Texture1dUnormSampleArgs {
            width: w,
            row_stride: rs,
            u,
            filter_abi: abi_f,
            wrap_x_abi: abi_wx,
        };

        let got = unsafe { texture1d_rgba16_unorm_sample(buf.as_ptr(), args) };
        assert_eq!(
            unsafe { texture1d_rgba16_unorm_sample(buf.as_ptr(), args) },
            got,
            "duplicate calls stable"
        );

        let ix = ref_nearest(u, w, wrap_x);
        let iy = 0u32;
        let exp = unsafe { load_rgba16_texel(buf.as_ptr(), texel_rel_byte_offset(ix, iy, rs, 8)) };
        assert_eq!(got, exp);
    }

    #[test]
    fn texture_rgba16_height_one_1d_ignores_implied_y_when_linear() {
        let w = 4u32;
        let rs = w * 8;
        let mut buf = alloc::vec![0u8; rs as usize];
        for ix in 0..w {
            let px = encode_rgba_texel((ix * 3000) as u16, 0, 0, 65535);
            let off = texel_rel_byte_offset(ix, 0, rs, 8);
            buf[off..off + 8].copy_from_slice(&px);
        }
        let u = uv(0.33);
        let abi_f = TextureFilter::Linear.to_builtin_abi();
        let abi_wx = TextureWrap::Repeat.to_builtin_abi();
        let args = Texture1dUnormSampleArgs {
            width: w,
            row_stride: rs,
            u,
            filter_abi: abi_f,
            wrap_x_abi: abi_wx,
        };
        let a = unsafe { texture1d_rgba16_unorm_sample(buf.as_ptr(), args) };
        let b = unsafe { texture1d_rgba16_unorm_sample(buf.as_ptr(), args) };
        assert_eq!(a, b);
    }

    #[test]
    fn texture_rgba16_1d_linear_matches_reference() {
        let w = 4u32;
        let rs = w * 8;
        let mut buf = alloc::vec![0u8; rs as usize];
        for ix in 0..w {
            let px = encode_rgba_texel((ix * 5000) as u16, 0, 0, 65535);
            let off = texel_rel_byte_offset(ix, 0, rs, 8);
            buf[off..off + 8].copy_from_slice(&px);
        }
        let u = uv(0.52);
        let ax = ref_linear(u, w, TextureWrap::ClampToEdge);
        let c0 = unsafe { load_rgba16_texel(buf.as_ptr(), texel_rel_byte_offset(ax.i0, 0, rs, 8)) };
        let c1 = unsafe { load_rgba16_texel(buf.as_ptr(), texel_rel_byte_offset(ax.i1, 0, rs, 8)) };
        let mut exp = [0i32; 4];
        for i in 0..4 {
            exp[i] = q32_lerp(c0[i], c1[i], ax.frac);
        }
        let got = unsafe {
            texture1d_rgba16_unorm_sample(
                buf.as_ptr(),
                Texture1dUnormSampleArgs {
                    width: w,
                    row_stride: rs,
                    u,
                    filter_abi: TextureFilter::Linear.to_builtin_abi(),
                    wrap_x_abi: TextureWrap::ClampToEdge.to_builtin_abi(),
                },
            )
        };
        assert_eq!(got, exp);
    }
}
