//! Texture format definitions
//!
//! Rgba16 is the default for new textures in the 16-bit pipeline. Rgba8 is retained
//! for backwards compatibility with legacy project state.

/// Texture pixel format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "UPPERCASE")] // Serializes as "RGB8", "RGBA8", "R8", "RGBA16"
pub enum TextureFormat {
    /// RGB 8-bit per channel (3 bytes per pixel)
    Rgb8,
    /// RGBA 8-bit per channel (4 bytes per pixel)
    Rgba8,
    /// Single channel 8-bit (1 byte per pixel)
    R8,
    /// RGBA 16-bit per channel (8 bytes per pixel), unsigned normalized
    Rgba16,
}

impl TextureFormat {
    /// Get bytes per pixel for this format
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            TextureFormat::Rgb8 => 3,
            TextureFormat::Rgba8 => 4,
            TextureFormat::R8 => 1,
            TextureFormat::Rgba16 => 8,
        }
    }

    /// Convert to string representation
    pub fn as_str(self) -> &'static str {
        match self {
            TextureFormat::Rgb8 => "RGB8",
            TextureFormat::Rgba8 => "RGBA8",
            TextureFormat::R8 => "R8",
            TextureFormat::Rgba16 => "RGBA16",
        }
    }

    /// Parse from string representation
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "RGB8" => Some(TextureFormat::Rgb8),
            "RGBA8" => Some(TextureFormat::Rgba8),
            "R8" => Some(TextureFormat::R8),
            "RGBA16" => Some(TextureFormat::Rgba16),
            _ => None,
        }
    }
}

impl Default for TextureFormat {
    fn default() -> Self {
        TextureFormat::Rgba16
    }
}

impl core::fmt::Display for TextureFormat {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}
