//! Shared sampler helpers: ABI decode, fixed-point lerp, texel offsets, safe loads.

use lps_q32::Q32;
use lps_shared::texture_format::{TextureFilter, TextureWrap};

/// Packed descriptor + normalized-coordinate arguments for 2D UNORM sampling (`base` passed separately).
#[derive(Clone, Copy)]
pub struct Texture2dUnormSampleArgs {
    pub width: u32,
    pub height: u32,
    pub row_stride: u32,
    pub u: i32,
    pub v: i32,
    pub filter_abi: u32,
    pub wrap_x_abi: u32,
    pub wrap_y_abi: u32,
}

/// Packed descriptor + coordinate arguments for 1D / height-one UNORM sampling (`base` passed separately).
#[derive(Clone, Copy)]
pub struct Texture1dUnormSampleArgs {
    pub width: u32,
    pub row_stride: u32,
    pub u: i32,
    pub filter_abi: u32,
    pub wrap_x_abi: u32,
}

/// Saturate `i64` to Q16.16 raw range (matches [`crate::builtins::texture::sample_ref`]).
#[inline]
pub(crate) fn sat_i64_to_q32_raw(wide: i64) -> i32 {
    const Q32_MAX_RAW: i64 = 0x7FFF_FFFF;
    if wide > Q32_MAX_RAW {
        Q32_MAX_RAW as i32
    } else if wide < i32::MIN as i64 {
        i32::MIN
    } else {
        wide as i32
    }
}

#[inline]
pub(crate) fn decode_filter_abi(abi: u32) -> TextureFilter {
    TextureFilter::from_builtin_abi(abi).unwrap_or(TextureFilter::Nearest)
}

#[inline]
pub(crate) fn decode_wrap_abi(abi: u32) -> TextureWrap {
    TextureWrap::from_builtin_abi(abi).unwrap_or(TextureWrap::ClampToEdge)
}

/// Linear interpolation on Q32-encoded scalar lanes; `frac_toward_b` is Q16.16 weight toward `b`.
#[inline]
pub(crate) fn q32_lerp(a: i32, b: i32, frac_toward_b: i32) -> i32 {
    let inv = Q32::ONE.to_fixed().wrapping_sub(frac_toward_b);
    let prod_a = (a as i64).wrapping_mul(inv as i64);
    let prod_b = (b as i64).wrapping_mul(frac_toward_b as i64);
    sat_i64_to_q32_raw((prod_a + prod_b) >> 16)
}

/// Byte offset of texel `(ix, iy)` relative to texture base pointer (`ptr` lane cast to `*const u8`).
#[inline]
pub(crate) fn texel_rel_byte_offset(ix: u32, iy: u32, row_stride: u32, bpp: u32) -> usize {
    iy as usize * row_stride as usize + ix as usize * bpp as usize
}

#[inline]
fn load_u16_unorm_q32_lane(base: *const u8, byte_off: usize) -> i32 {
    unsafe {
        let raw = core::ptr::read_unaligned(base.add(byte_off).cast::<u16>());
        crate::builtins::lpir::unorm_conv_q32::__lp_lpir_unorm16_to_f_q32(raw as i32)
    }
}

/// Four RGBA16 channels starting at `texel_byte_off` bytes from `base`.
///
/// # Safety
/// `base.add(texel_byte_off)` must be valid for an 8-byte unaligned read.
pub(crate) unsafe fn load_rgba16_texel(base: *const u8, texel_byte_off: usize) -> [i32; 4] {
    [
        load_u16_unorm_q32_lane(base, texel_byte_off),
        load_u16_unorm_q32_lane(base, texel_byte_off + 2),
        load_u16_unorm_q32_lane(base, texel_byte_off + 4),
        load_u16_unorm_q32_lane(base, texel_byte_off + 6),
    ]
}

/// Single R16 channel at `texel_byte_off`; caller fills GB + A per format rules.
///
/// # Safety
/// `base.add(texel_byte_off)` must be valid for a 2-byte unaligned read.
#[inline]
pub(crate) unsafe fn load_r16_texel_lane(base: *const u8, texel_byte_off: usize) -> i32 {
    // Same halfword load as one RGBA channel.
    load_u16_unorm_q32_lane(base, texel_byte_off)
}
