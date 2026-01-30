use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Serialize};

/// Texture node state - runtime values
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureState {
    /// Texture pixel data
    pub texture_data: Vec<u8>,
    /// Texture width in pixels
    pub width: u32,
    /// Texture height in pixels
    pub height: u32,
    /// Texture format (e.g., "RGB8", "RGBA8", "R8")
    pub format: String,
}
