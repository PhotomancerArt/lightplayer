# Phase 1 — Add TextureStorageFormat + TextureBuffer to lps-shared

## Scope

Add two new files to `lps-shared`: the `TextureStorageFormat` enum and the
`TextureBuffer` trait. Wire them into `lib.rs` with public re-exports.

## Code organization reminders

- One concept per file.
- Place traits and public API first, helpers at the bottom.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation details

### `lps-shared/src/texture_format.rs`

```rust
/// Storage format for CPU-side texture data.
///
/// Single variant for now — the enum exists so format is explicit in the API
/// rather than implicit. Future variants added when there is a concrete consumer.
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
```

Use `match` rather than hard-coded constants so the compiler forces updates
when new variants are added.

### `lps-shared/src/texture_buffer.rs`

```rust
use crate::texture_format::TextureStorageFormat;

/// Read/write access to a 2D pixel buffer.
///
/// Concrete implementations live in higher-level crates (e.g. `lp-shader`).
/// This trait is in `lps-shared` so types that only need the abstraction
/// don't pull in runtime dependencies.
pub trait TextureBuffer {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn format(&self) -> TextureStorageFormat;

    /// Raw byte slice covering all pixels, row-major, tightly packed.
    fn data(&self) -> &[u8];

    /// Mutable byte slice covering all pixels.
    fn data_mut(&mut self) -> &mut [u8];
}
```

### `lps-shared/src/lib.rs` updates

Add the two new modules and re-export the public types:

```rust
pub mod texture_buffer;
pub mod texture_format;

pub use texture_buffer::TextureBuffer;
pub use texture_format::TextureStorageFormat;
```

### Tests

Add to `texture_format.rs`:

```rust
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
```

## Validate

```bash
cargo check -p lps-shared
cargo test -p lps-shared
```
