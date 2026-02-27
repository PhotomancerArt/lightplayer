//! Texture format constants and utilities

// Re-export TextureFormat from lp-model
pub use lp_model::nodes::texture::TextureFormat;

// Backward compatibility: Keep constants for migration period
/// RGB8 format constant (deprecated: use TextureFormat::Rgb8)
pub const RGB8: &str = "RGB8";

/// RGBA8 format constant (deprecated: use TextureFormat::Rgba8)
pub const RGBA8: &str = "RGBA8";

/// R8 format constant (deprecated: use TextureFormat::R8)
pub const R8: &str = "R8";

/// Check if a format string is valid (deprecated: use TextureFormat::from_str)
pub fn is_valid(format: &str) -> bool {
    TextureFormat::from_str(format).is_some()
}

/// Get bytes per pixel for a format (deprecated: use TextureFormat::bytes_per_pixel)
pub fn bytes_per_pixel(format: &str) -> Option<usize> {
    TextureFormat::from_str(format).map(|f| f.bytes_per_pixel())
}
