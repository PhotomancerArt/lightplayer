# Phase 4 — Implement LpsTextureBuf + alloc_texture

## Scope

Implement `LpsTextureBuf` (wrapping `LpvmBuffer` + dimensions + format) and
add `LpsEngine::alloc_texture()`. The texture is allocated from the engine's
`LpvmMemory` so it's guest-addressable for future in-shader reads (M3).

## Code organization reminders

- One concept per file.
- Place traits and public API first, helpers at the bottom.
- Any temporary code should have a TODO comment.

## Implementation details

### `lp-shader/lp-shader/src/texture_buf.rs`

Replace the Phase 3 stub with the full implementation:

```rust
use lpvm::LpvmBuffer;
use lps_shared::{TextureBuffer, TextureStorageFormat};

/// Pixel buffer backed by a shared-memory allocation (`LpvmBuffer`).
///
/// Allocated via [`LpsEngine::alloc_texture`](crate::LpsEngine::alloc_texture).
/// The underlying memory is guest-addressable, so shaders can read from
/// it in future milestones (texture sampling).
pub struct LpsTextureBuf {
    buffer: LpvmBuffer,
    width: u32,
    height: u32,
    format: TextureStorageFormat,
}

impl LpsTextureBuf {
    pub(crate) fn new(
        buffer: LpvmBuffer,
        width: u32,
        height: u32,
        format: TextureStorageFormat,
    ) -> Self {
        Self { buffer, width, height, format }
    }

    /// Guest-visible base pointer for passing to shaders as a uniform.
    pub fn guest_ptr(&self) -> lpvm::LpvmPtr {
        self.buffer.as_ptr()
    }

    /// Row stride in bytes (tightly packed, no padding).
    pub fn row_stride(&self) -> usize {
        self.width as usize * self.format.bytes_per_pixel()
    }
}

impl TextureBuffer for LpsTextureBuf {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn format(&self) -> TextureStorageFormat {
        self.format
    }

    fn data(&self) -> &[u8] {
        let size = self.width as usize
            * self.height as usize
            * self.format.bytes_per_pixel();
        unsafe {
            core::slice::from_raw_parts(self.buffer.native_ptr(), size)
        }
    }

    fn data_mut(&mut self) -> &mut [u8] {
        let size = self.width as usize
            * self.height as usize
            * self.format.bytes_per_pixel();
        unsafe {
            core::slice::from_raw_parts_mut(self.buffer.native_ptr(), size)
        }
    }
}
```

### `LpsEngine::alloc_texture` — add to `engine.rs`

Add this method to the existing `impl<E: LpvmEngine> LpsEngine<E>` block:

```rust
use lpvm::AllocError;

/// Allocate a texture in the engine's shared memory.
///
/// The returned buffer is zeroed and guest-addressable.
pub fn alloc_texture(
    &self,
    width: u32,
    height: u32,
    format: TextureStorageFormat,
) -> Result<LpsTextureBuf, AllocError> {
    let size = width as usize * height as usize * format.bytes_per_pixel();
    let align = 4; // u16 channels, 4-byte align is sufficient
    let buffer = self.engine.memory().alloc(size, align)?;
    Ok(LpsTextureBuf::new(buffer, width, height, format))
}
```

### `lib.rs` updates

Add the public export:

```rust
pub use texture_buf::LpsTextureBuf;
```

Also re-export `AllocError` from lpvm for consumers:

```rust
pub use lpvm::AllocError;
```

## Validate

```bash
cargo check -p lp-shader
cargo test -p lp-shader
cargo check  # full default workspace
```
