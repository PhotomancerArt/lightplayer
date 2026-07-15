//! Linear RGBA16 → sRGB RGBA8 conversion for canvas presentation.
//!
//! The engine materializes visual products as linear `Rgba16Unorm` textures;
//! browser canvases want 8-bit sRGB. The transfer curve matches the wire
//! render-product probe (`lpc-engine` `project_read_probes`), evaluated once
//! into a full 16-bit lookup table so per-frame conversion is a table walk.

use std::sync::OnceLock;

/// Convert tightly packed linear RGBA16 (little-endian) pixels to sRGB RGBA8.
///
/// Alpha is forced opaque: preview cards composite onto the page and the
/// product alpha channel is not meaningful for presentation.
pub(crate) fn rgba16_unorm_to_rgba8_srgb(rgba16: &[u8]) -> Vec<u8> {
    let lut = srgb8_lut();
    let mut out = Vec::with_capacity(rgba16.len() / 8 * 4);
    for px in rgba16.chunks_exact(8) {
        out.push(lut[u16::from_le_bytes([px[0], px[1]]) as usize]);
        out.push(lut[u16::from_le_bytes([px[2], px[3]]) as usize]);
        out.push(lut[u16::from_le_bytes([px[4], px[5]]) as usize]);
        out.push(0xff);
    }
    out
}

/// Full linear-unorm16 → sRGB-u8 table, built on first use.
fn srgb8_lut() -> &'static [u8; 65536] {
    static LUT: OnceLock<Box<[u8; 65536]>> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut lut = Box::new([0u8; 65536]);
        for (value, out) in lut.iter_mut().enumerate() {
            *out = linear_unorm16_to_srgb8(value as u16);
        }
        lut
    })
}

fn linear_unorm16_to_srgb8(value: u16) -> u8 {
    let linear = value as f32 / u16::MAX as f32;
    let srgb = if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (srgb.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_black_and_white_endpoints() {
        let mut px = Vec::new();
        px.extend_from_slice(&[0u8; 8]); // black, alpha 0
        px.extend_from_slice(&0xffffu16.to_le_bytes().repeat(4)); // white, alpha max

        let out = rgba16_unorm_to_rgba8_srgb(&px);

        assert_eq!(out, vec![0, 0, 0, 255, 255, 255, 255, 255]);
    }

    #[test]
    fn mid_gray_is_gamma_encoded() {
        let mid = 0x8000u16.to_le_bytes();
        let px = [mid, mid, mid, mid].concat();

        let out = rgba16_unorm_to_rgba8_srgb(&px);

        // Linear 0.5 encodes to ~0.7354 sRGB (188/255), not 128.
        assert_eq!(out[0], 188);
        assert_eq!(out[3], 255);
    }
}
