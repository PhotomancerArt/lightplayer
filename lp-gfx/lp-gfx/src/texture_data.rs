//! Owned CPU-side texel bytes read back from a texture handle.

use alloc::vec::Vec;

use lps_shared::TextureStorageFormat;

/// Owned texel bytes plus dimensions/format, produced by
/// [`crate::LpGraphics::read_back`].
///
/// The contract is "bytes come back": regardless of where the backend keeps
/// the texture resident, `read_back` yields tightly packed little-endian
/// texel bytes the CPU can consume directly.
#[derive(Clone, Debug)]
pub struct TextureData {
    width: u32,
    height: u32,
    format: TextureStorageFormat,
    bytes: Vec<u8>,
}

impl TextureData {
    /// Assemble read-back data. **Backend-facing**: `bytes` must be tightly
    /// packed `width × height × bytes_per_pixel`.
    pub fn new(width: u32, height: u32, format: TextureStorageFormat, bytes: Vec<u8>) -> Self {
        debug_assert_eq!(
            bytes.len(),
            width as usize * height as usize * format.bytes_per_pixel(),
        );
        Self {
            width,
            height,
            format,
            bytes,
        }
    }

    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn format(&self) -> TextureStorageFormat {
        self.format
    }

    /// Tightly packed texel bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Consume into the texel byte vector.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}
