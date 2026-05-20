//! Shader sample buffers backed by [`lpvm::LpvmBuffer`] shared memory.

use lpvm::LpvmBuffer;

/// Packed Q16.16 shader pixel-space sample points.
///
/// Each point is `[x_pixel_q16, y_pixel_q16]`. These are the same continuous
/// pixel coordinates passed to `render(vec2 pos)`, not normalized texture UVs.
pub struct LpsSamplePointBuf {
    buffer: LpvmBuffer,
    count: u32,
}

impl LpsSamplePointBuf {
    pub(crate) fn new(buffer: LpvmBuffer, count: u32) -> Self {
        debug_assert_eq!(buffer.size(), count as usize * 8);
        Self { buffer, count }
    }

    #[must_use]
    pub fn count(&self) -> u32 {
        self.count
    }

    #[must_use]
    pub fn buffer(&self) -> LpvmBuffer {
        self.buffer
    }

    #[must_use]
    pub fn data(&self) -> &[i32] {
        unsafe {
            core::slice::from_raw_parts(self.buffer.native_ptr().cast(), self.count as usize * 2)
        }
    }

    #[must_use]
    pub fn data_mut(&mut self) -> &mut [i32] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self.buffer.native_ptr().cast(),
                self.count as usize * 2,
            )
        }
    }
}

/// Packed RGBA16 visual sample colors.
pub struct LpsSampleRgba16Buf {
    buffer: LpvmBuffer,
    count: u32,
}

impl LpsSampleRgba16Buf {
    pub(crate) fn new(buffer: LpvmBuffer, count: u32) -> Self {
        debug_assert_eq!(buffer.size(), count as usize * 8);
        Self { buffer, count }
    }

    #[must_use]
    pub fn count(&self) -> u32 {
        self.count
    }

    #[must_use]
    pub fn buffer(&self) -> LpvmBuffer {
        self.buffer
    }

    #[must_use]
    pub fn data(&self) -> &[u16] {
        unsafe {
            core::slice::from_raw_parts(self.buffer.native_ptr().cast(), self.count as usize * 4)
        }
    }

    #[must_use]
    pub fn data_mut(&mut self) -> &mut [u16] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self.buffer.native_ptr().cast(),
                self.count as usize * 4,
            )
        }
    }
}

unsafe impl Send for LpsSamplePointBuf {}
unsafe impl Sync for LpsSamplePointBuf {}
unsafe impl Send for LpsSampleRgba16Buf {}
unsafe impl Sync for LpsSampleRgba16Buf {}
