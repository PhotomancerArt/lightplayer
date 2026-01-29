//! RGB8 format sampler

use super::TextureSampler;

pub struct Rgb8Sampler;

impl TextureSampler for Rgb8Sampler {
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

        let bytes_per_pixel = 3;
        let offset = ((y * width + x) as usize) * bytes_per_pixel;

        if offset + bytes_per_pixel > data.len() {
            return None;
        }

        Some([data[offset], data[offset + 1], data[offset + 2]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_sample_pixel() {
        let sampler = Rgb8Sampler;
        // Create a 2x2 RGB8 texture: [R,G,B, R,G,B, R,G,B, R,G,B]
        let data = vec![
            255, 0, 0, // Pixel (0,0): Red
            0, 255, 0, // Pixel (1,0): Green
            0, 0, 255, // Pixel (0,1): Blue
            255, 255, 255, // Pixel (1,1): White
        ];

        assert_eq!(sampler.sample_pixel(&data, 0, 0, 2, 2), Some([255, 0, 0]));
        assert_eq!(sampler.sample_pixel(&data, 1, 0, 2, 2), Some([0, 255, 0]));
        assert_eq!(sampler.sample_pixel(&data, 0, 1, 2, 2), Some([0, 0, 255]));
        assert_eq!(
            sampler.sample_pixel(&data, 1, 1, 2, 2),
            Some([255, 255, 255])
        );
    }

    #[test]
    fn test_out_of_bounds() {
        let sampler = Rgb8Sampler;
        let data = vec![255, 0, 0, 0, 255, 0];

        assert_eq!(sampler.sample_pixel(&data, 2, 0, 2, 1), None);
        assert_eq!(sampler.sample_pixel(&data, 0, 1, 2, 1), None);
    }
}
