//! Trait for read/write access to a 2D pixel buffer.

use crate::texture_format::TextureStorageFormat;

/// Read/write access to a 2D pixel buffer.
///
/// Concrete implementations live in higher-level crates (e.g. `lp-shader`).
/// This trait is in `lps-shared` so callers only need the abstraction without
/// pulling in the full runtime.
pub trait TextureBuffer {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn format(&self) -> TextureStorageFormat;

    /// Raw byte slice covering all pixels, row-major, tightly packed.
    fn data(&self) -> &[u8];

    /// Mutable byte slice covering all pixels.
    fn data_mut(&mut self) -> &mut [u8];
}
