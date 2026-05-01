//! RGBA16 format sampler

use super::TextureSampler;

pub struct Rgba16Sampler;

impl TextureSampler for Rgba16Sampler {
    fn sample_pixel(
        &self,
        data: &[u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Option<[u8; 3]> {
        if x >= width || y >= height {
            return None;
        }

        let bytes_per_pixel = 8;
        let offset = ((y * width + x) as usize) * bytes_per_pixel;

        if offset + bytes_per_pixel > data.len() {
            return None;
        }

        let r = u16::from_le_bytes([data[offset], data[offset + 1]]);
        let g = u16::from_le_bytes([data[offset + 2], data[offset + 3]]);
        let b = u16::from_le_bytes([data[offset + 4], data[offset + 5]]);

        Some([(r >> 8) as u8, (g >> 8) as u8, (b >> 8) as u8])
    }
}
