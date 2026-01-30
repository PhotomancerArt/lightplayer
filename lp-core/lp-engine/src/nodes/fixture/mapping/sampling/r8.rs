//! R8 (grayscale) format sampler

use super::TextureSampler;

pub struct R8Sampler;

impl TextureSampler for R8Sampler {
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

        let bytes_per_pixel = 1;
        let offset = ((y * width + x) as usize) * bytes_per_pixel;

        if offset + bytes_per_pixel > data.len() {
            return None;
        }

        // Grayscale: R=G=B
        let gray = data[offset];
        Some([gray, gray, gray])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_sample_pixel() {
        let sampler = R8Sampler;
        // Create a 2x2 R8 texture: [gray, gray, gray, gray]
        let data = vec![
            128, // Pixel (0,0)
            64,  // Pixel (1,0)
            192, // Pixel (0,1)
            255, // Pixel (1,1)
        ];

        assert_eq!(
            sampler.sample_pixel(&data, 0, 0, 2, 2),
            Some([128, 128, 128])
        );
        assert_eq!(sampler.sample_pixel(&data, 1, 0, 2, 2), Some([64, 64, 64]));
        assert_eq!(
            sampler.sample_pixel(&data, 0, 1, 2, 2),
            Some([192, 192, 192])
        );
        assert_eq!(
            sampler.sample_pixel(&data, 1, 1, 2, 2),
            Some([255, 255, 255])
        );
    }

    #[test]
    fn test_out_of_bounds() {
        let sampler = R8Sampler;
        let data = vec![128, 64];

        assert_eq!(sampler.sample_pixel(&data, 2, 0, 2, 1), None);
        assert_eq!(sampler.sample_pixel(&data, 0, 1, 2, 1), None);
    }
}
