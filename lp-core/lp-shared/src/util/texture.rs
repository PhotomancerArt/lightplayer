//! Low-level texture abstraction for pixel buffer management

use crate::error::TextureError;
use crate::util::formats::TextureFormat;

/// Texture structure for managing pixel buffers
#[derive(Debug, Clone)]
pub struct Texture {
    width: u32,
    height: u32,
    format: TextureFormat,
    data: alloc::vec::Vec<u8>,
}

impl Texture {
    /// Create a new texture with the given dimensions and format
    ///
    /// Allocates buffer and initializes to zeros.
    pub fn new(width: u32, height: u32, format: TextureFormat) -> Result<Self, TextureError> {
        let bytes_per_pixel = format.bytes_per_pixel();

        let buffer_size = (width as usize)
            .checked_mul(height as usize)
            .and_then(|size| size.checked_mul(bytes_per_pixel))
            .ok_or_else(|| TextureError::DimensionsTooLarge { width, height })?;

        let data = alloc::vec::Vec::with_capacity(buffer_size);
        // Initialize to zeros
        let mut data = data;
        data.resize(buffer_size, 0);

        Ok(Self {
            width,
            height,
            format,
            data,
        })
    }

    /// Get the format
    pub fn format(&self) -> TextureFormat {
        self.format
    }

    /// Get bytes per pixel for this texture's format
    pub fn bytes_per_pixel(&self) -> usize {
        self.format.bytes_per_pixel()
    }

    /// Get the width
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Get the height
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Get a pixel value at the given coordinates
    ///
    /// Returns RGBA values as [u8; 4], with missing channels set to 0.
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }

        let bytes_per_pixel = self.bytes_per_pixel();
        let offset = ((y * self.width + x) as usize) * bytes_per_pixel;

        if offset + bytes_per_pixel > self.data.len() {
            return None;
        }

        let mut result = [0u8; 4];
        match self.format {
            TextureFormat::Rgb8 => {
                result[0] = self.data[offset];
                result[1] = self.data[offset + 1];
                result[2] = self.data[offset + 2];
                result[3] = 255; // Alpha defaults to 255
            }
            TextureFormat::Rgba8 => {
                result[0] = self.data[offset];
                result[1] = self.data[offset + 1];
                result[2] = self.data[offset + 2];
                result[3] = self.data[offset + 3];
            }
            TextureFormat::R8 => {
                result[0] = self.data[offset];
                result[1] = self.data[offset]; // Grayscale: R=G=B
                result[2] = self.data[offset];
                result[3] = 255; // Alpha defaults to 255
            }
        }

        Some(result)
    }

    /// Set a pixel value at the given coordinates
    ///
    /// Takes RGBA values as [u8; 4], but only writes relevant bytes based on format:
    /// - RGB8: writes first 3 bytes
    /// - RGBA8: writes all 4 bytes
    /// - R8: writes first byte only
    pub fn set_pixel(&mut self, x: u32, y: u32, color: [u8; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }

        let bytes_per_pixel = self.bytes_per_pixel();
        let offset = ((y * self.width + x) as usize) * bytes_per_pixel;

        if offset + bytes_per_pixel > self.data.len() {
            return;
        }

        match self.format {
            TextureFormat::Rgb8 => {
                self.data[offset] = color[0];
                self.data[offset + 1] = color[1];
                self.data[offset + 2] = color[2];
            }
            TextureFormat::Rgba8 => {
                self.data[offset] = color[0];
                self.data[offset + 1] = color[1];
                self.data[offset + 2] = color[2];
                self.data[offset + 3] = color[3];
            }
            TextureFormat::R8 => {
                self.data[offset] = color[0];
            }
        }
    }

    /// Sample the texture at normalized coordinates (u, v) in [0, 1]
    ///
    /// Uses bilinear sampling.
    pub fn sample(&self, u: f32, v: f32) -> Option<[u8; 4]> {
        // Clamp coordinates to [0, 1]
        let u = u.max(0.0).min(1.0);
        let v = v.max(0.0).min(1.0);

        // Convert to pixel coordinates
        let x = u * (self.width as f32 - 1.0);
        let y = v * (self.height as f32 - 1.0);

        // Get integer coordinates for bilinear sampling (manual floor)
        let x0 = x as u32;
        let y0 = y as u32;
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);

        // Get fractional parts
        let fx = x - x0 as f32;
        let fy = y - y0 as f32;

        // Sample four corners
        let p00 = self.get_pixel(x0, y0)?;
        let p10 = self.get_pixel(x1, y0)?;
        let p01 = self.get_pixel(x0, y1)?;
        let p11 = self.get_pixel(x1, y1)?;

        // Bilinear interpolation
        let lerp = |a: u8, b: u8, t: f32| -> u8 { (a as f32 * (1.0 - t) + b as f32 * t) as u8 };

        let top = [
            lerp(p00[0], p10[0], fx),
            lerp(p00[1], p10[1], fx),
            lerp(p00[2], p10[2], fx),
            lerp(p00[3], p10[3], fx),
        ];
        let bottom = [
            lerp(p01[0], p11[0], fx),
            lerp(p01[1], p11[1], fx),
            lerp(p01[2], p11[2], fx),
            lerp(p01[3], p11[3], fx),
        ];

        Some([
            lerp(top[0], bottom[0], fy),
            lerp(top[1], bottom[1], fy),
            lerp(top[2], bottom[2], fy),
            lerp(top[3], bottom[3], fy),
        ])
    }

    /// Compute all pixels using a function
    ///
    /// The function receives (x, y) coordinates and returns RGBA [u8; 4].
    pub fn compute_all<F>(&mut self, f: F)
    where
        F: Fn(u32, u32) -> [u8; 4],
    {
        for y in 0..self.height {
            for x in 0..self.width {
                let color = f(x, y);
                self.set_pixel(x, y, color);
            }
        }
    }

    /// Get raw pixel data (for advanced use cases)
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get mutable raw pixel data (for advanced use cases)
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::formats::TextureFormat;

    #[test]
    fn test_texture_new_valid() {
        let texture = Texture::new(64, 64, TextureFormat::Rgb8).unwrap();
        assert_eq!(texture.width(), 64);
        assert_eq!(texture.height(), 64);
        assert_eq!(texture.format(), TextureFormat::Rgb8);
        assert_eq!(texture.bytes_per_pixel(), 3);
    }

    #[test]
    fn test_texture_buffer_initialized_to_zeros() {
        let texture = Texture::new(10, 10, TextureFormat::Rgb8).unwrap();
        // Check that buffer is initialized to zeros
        assert_eq!(texture.data()[0], 0);
        assert_eq!(texture.data()[texture.data().len() - 1], 0);
    }

    #[test]
    fn test_get_set_pixel_rgb8() {
        let mut texture = Texture::new(10, 10, TextureFormat::Rgb8).unwrap();
        texture.set_pixel(5, 5, [100, 200, 255, 0]);
        let pixel = texture.get_pixel(5, 5).unwrap();
        assert_eq!(pixel[0], 100);
        assert_eq!(pixel[1], 200);
        assert_eq!(pixel[2], 255);
        assert_eq!(pixel[3], 255); // Alpha defaults to 255 for RGB8
    }

    #[test]
    fn test_get_set_pixel_rgba8() {
        let mut texture = Texture::new(10, 10, TextureFormat::Rgba8).unwrap();
        texture.set_pixel(5, 5, [100, 200, 255, 128]);
        let pixel = texture.get_pixel(5, 5).unwrap();
        assert_eq!(pixel, [100, 200, 255, 128]);
    }

    #[test]
    fn test_get_set_pixel_r8() {
        let mut texture = Texture::new(10, 10, TextureFormat::R8).unwrap();
        texture.set_pixel(5, 5, [128, 0, 0, 0]); // Only first byte matters
        let pixel = texture.get_pixel(5, 5).unwrap();
        assert_eq!(pixel[0], 128);
        assert_eq!(pixel[1], 128); // Grayscale: R=G=B
        assert_eq!(pixel[2], 128);
        assert_eq!(pixel[3], 255); // Alpha defaults to 255
    }

    #[test]
    fn test_get_pixel_out_of_bounds() {
        let texture = Texture::new(10, 10, TextureFormat::Rgb8).unwrap();
        assert!(texture.get_pixel(10, 5).is_none());
        assert!(texture.get_pixel(5, 10).is_none());
    }

    #[test]
    fn test_sample() {
        let mut texture = Texture::new(2, 2, TextureFormat::Rgb8).unwrap();
        // Set corners to different colors
        texture.set_pixel(0, 0, [255, 0, 0, 255]); // Red
        texture.set_pixel(1, 0, [0, 255, 0, 255]); // Green
        texture.set_pixel(0, 1, [0, 0, 255, 255]); // Blue
        texture.set_pixel(1, 1, [255, 255, 255, 255]); // White

        // Sample at corner (should be exact)
        let pixel = texture.sample(0.0, 0.0).unwrap();
        assert_eq!(pixel[0], 255);
        assert_eq!(pixel[1], 0);
        assert_eq!(pixel[2], 0);

        // Sample at center (should be interpolated)
        let pixel = texture.sample(0.5, 0.5).unwrap();
        // Should be some blend of all four corners
        assert!(pixel[0] > 0);
        assert!(pixel[1] > 0);
        assert!(pixel[2] > 0);
    }

    #[test]
    fn test_compute_all() {
        let mut texture = Texture::new(10, 10, TextureFormat::Rgb8).unwrap();
        texture.compute_all(|x, y| [(x * 10) as u8, (y * 10) as u8, 128, 255]);

        // Check a few pixels
        let pixel = texture.get_pixel(5, 3).unwrap();
        assert_eq!(pixel[0], 50);
        assert_eq!(pixel[1], 30);
        assert_eq!(pixel[2], 128);
    }
}
