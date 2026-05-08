//! Request for materializing a render product into a complete texture.

use lps_shared::TextureStorageFormat;

/// Texture render request issued by a consumer that needs a full materialized frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderTextureRequest {
    pub width: u32,
    pub height: u32,
    pub format: TextureStorageFormat,
    pub time_seconds: f32,
}
