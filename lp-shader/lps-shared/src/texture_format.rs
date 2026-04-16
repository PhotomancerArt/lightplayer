//! CPU-side texture storage format (single source of truth for layout).

/// Storage format for CPU-side texture data.
///
/// Single variant for now — the enum exists so format is explicit in the API
/// rather than implicit. Future variants are added when there is a concrete consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureStorageFormat {
    /// RGBA 16-bit unsigned normalized, 8 bytes/pixel.
    ///
    /// Each channel is a `u16` in `[0, 65535]` representing `[0.0, 1.0]`.
    /// Q32 fractional bits map to unorm16 via saturate: `min(q32, 65535)`.
    Rgba16Unorm,
}

impl TextureStorageFormat {
    #[inline]
    #[must_use]
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgba16Unorm => 8,
        }
    }

    #[inline]
    #[must_use]
    pub fn channel_count(self) -> usize {
        match self {
            Self::Rgba16Unorm => 4,
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
}
