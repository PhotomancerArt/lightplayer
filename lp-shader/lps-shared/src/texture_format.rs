//! CPU-side texture storage format (single source of truth for layout).

/// Storage format for CPU-side texture data.
///
/// Variants are added when there is a concrete consumer; each variant documents
/// layout and channel semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureStorageFormat {
    /// RGBA 16-bit unsigned normalized, 8 bytes/pixel.
    ///
    /// Each channel is a `u16` in `[0, 65535]` representing `[0.0, 1.0]`.
    /// Q32 fractional bits map to unorm16 via saturate: `min(q32, 65535)`.
    Rgba16Unorm,
    /// RGB 16-bit unsigned normalized, 6 bytes/pixel (no alpha).
    ///
    /// Tightly packed: 3 × u16 = 6 bytes per pixel. No padding.
    Rgb16Unorm,
    /// Single-channel 16-bit unsigned normalized, 2 bytes/pixel.
    R16Unorm,
}

impl TextureStorageFormat {
    #[inline]
    #[must_use]
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgba16Unorm => 8,
            Self::Rgb16Unorm => 6,
            Self::R16Unorm => 2,
        }
    }

    #[inline]
    #[must_use]
    pub fn channel_count(self) -> usize {
        match self {
            Self::Rgba16Unorm => 4,
            Self::Rgb16Unorm => 3,
            Self::R16Unorm => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba16_unorm_bytes_per_pixel() {
        assert_eq!(TextureStorageFormat::Rgba16Unorm.bytes_per_pixel(), 8);
    }

    #[test]
    fn rgba16_unorm_channel_count() {
        assert_eq!(TextureStorageFormat::Rgba16Unorm.channel_count(), 4);
    }

    #[test]
    fn rgb16_unorm_metrics() {
        assert_eq!(TextureStorageFormat::Rgb16Unorm.bytes_per_pixel(), 6);
        assert_eq!(TextureStorageFormat::Rgb16Unorm.channel_count(), 3);
    }

    #[test]
    fn r16_unorm_metrics() {
        assert_eq!(TextureStorageFormat::R16Unorm.bytes_per_pixel(), 2);
        assert_eq!(TextureStorageFormat::R16Unorm.channel_count(), 1);
    }
}
