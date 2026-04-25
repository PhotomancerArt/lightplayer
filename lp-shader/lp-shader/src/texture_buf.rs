//! Texture buffer backed by [`lpvm::LpvmBuffer`] shared memory.

use lps_shared::{TextureBuffer, TextureStorageFormat};
use lpvm::LpvmBuffer;
use lpvm::LpvmPtr;

/// Pixel buffer backed by a shared-memory allocation ([`LpvmBuffer`]).
///
/// Allocated via [`crate::LpsEngine::alloc_texture`]. The memory is guest-addressable
/// so shaders can read from it in future milestones.
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
        debug_assert!(buffer.size() == width as usize * height as usize * format.bytes_per_pixel());
        Self {
            buffer,
            width,
            height,
            format,
        }
    }

    /// Guest-visible base pointer for passing to shaders as a uniform.
    #[must_use]
    pub fn guest_ptr(&self) -> LpvmPtr {
        self.buffer.as_ptr()
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

    /// Row stride in bytes (tightly packed, no padding).
    #[must_use]
    pub fn row_stride(&self) -> usize {
        self.width as usize * self.format.bytes_per_pixel()
    }

    /// Underlying shared allocation (host pointer, size, guest base).
    #[must_use]
    pub fn buffer(&self) -> LpvmBuffer {
        self.buffer
    }
}

impl TextureBuffer for LpsTextureBuf {
    fn width(&self) -> u32 {
        LpsTextureBuf::width(self)
    }

    fn height(&self) -> u32 {
        LpsTextureBuf::height(self)
    }

    fn format(&self) -> TextureStorageFormat {
        LpsTextureBuf::format(self)
    }

    fn data(&self) -> &[u8] {
        let len = self.buffer.size();
        unsafe { core::slice::from_raw_parts(self.buffer.native_ptr(), len) }
    }

    fn data_mut(&mut self) -> &mut [u8] {
        let len = self.buffer.size();
        unsafe { core::slice::from_raw_parts_mut(self.buffer.native_ptr(), len) }
    }
}

// SAFETY: LPVM buffers are owned by the embedding engine's memory pool; LightPlayer
// renders the node graph on one thread per runtime, so no concurrent access.
unsafe impl Send for LpsTextureBuf {}
unsafe impl Sync for LpsTextureBuf {}
