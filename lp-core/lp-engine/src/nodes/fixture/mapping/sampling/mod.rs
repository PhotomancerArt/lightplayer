//! Format-specific texture sampling

pub mod r8;
pub mod rgb8;
pub mod rgba8;

use alloc::{boxed::Box, vec::Vec};

/// Trait for format-specific texture sampling
pub trait TextureSampler {
    /// Sample a single pixel from texture data
    ///
    /// Returns RGB values as [u8; 3], or None if coordinates are out of bounds
    fn sample_pixel(&self, data: &[u8], x: u32, y: u32, width: u32, height: u32)
    -> Option<[u8; 3]>;

    /// Sample multiple pixels in batch
    ///
    /// Returns a vector of RGB values, with None for out-of-bounds pixels
    fn sample_batch(
        &self,
        data: &[u8],
        pixels: &[(u32, u32)],
        width: u32,
        height: u32,
    ) -> Vec<Option<[u8; 3]>> {
        pixels
            .iter()
            .map(|(x, y)| self.sample_pixel(data, *x, *y, width, height))
            .collect()
    }
}

/// Create a sampler for the given texture format
pub fn create_sampler(format: &str) -> Option<Box<dyn TextureSampler>> {
    match format {
        lp_shared::util::formats::RGB8 => Some(Box::new(rgb8::Rgb8Sampler)),
        lp_shared::util::formats::RGBA8 => Some(Box::new(rgba8::Rgba8Sampler)),
        lp_shared::util::formats::R8 => Some(Box::new(r8::R8Sampler)),
        _ => None,
    }
}
