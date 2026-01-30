//! Texture format constants and utilities

/// RGB8 format constant
pub const RGB8: &str = "RGB8";

/// RGBA8 format constant
pub const RGBA8: &str = "RGBA8";

/// R8 format constant
pub const R8: &str = "R8";

/// Check if a format string is valid
pub fn is_valid(format: &str) -> bool {
    matches!(format, RGB8 | RGBA8 | R8)
}

/// Get bytes per pixel for a format
pub fn bytes_per_pixel(format: &str) -> Option<usize> {
    match format {
        RGB8 => Some(3),
        RGBA8 => Some(4),
        R8 => Some(1),
        _ => None,
    }
}
