//! Host/guest shared memory allocation types.
//!
//! [`LpvmBuffer`] is an owning allocation handle with base pointer, size, and guest offset.
//! [`LpvmPtr`] is a non-owning pointer (guest offset) suitable for addressing within a buffer
//! or passing to shaders as uniforms.

use core::fmt;

/// Owning handle to a host↔guest shared allocation.
///
/// Contains the host base pointer, guest-visible offset, size, and alignment.
/// Dropping does **not** free the memory; call [`LpvmMemory::free`](crate::LpvmMemory) explicitly.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LpvmBuffer {
    native: *mut u8,
    guest: u64,
    size: usize,
    align: usize,
}

impl LpvmBuffer {
    /// Build a buffer handle. Creating this value is safe; dereferencing `native`
    /// is only safe when the allocation is valid and synchronized.
    #[inline]
    pub const fn new(native: *mut u8, guest: u64, size: usize, align: usize) -> Self {
        Self {
            native,
            guest,
            size,
            align,
        }
    }

    /// Host base pointer for this allocation.
    #[inline]
    pub fn native_ptr(self) -> *mut u8 {
        self.native
    }

    /// Guest-visible base offset (for uniforms).
    #[inline]
    pub fn guest_base(self) -> u64 {
        self.guest
    }

    /// Size of the allocation in bytes.
    #[inline]
    pub fn size(self) -> usize {
        self.size
    }

    /// Alignment of the allocation.
    #[inline]
    pub fn align(self) -> usize {
        self.align
    }

    /// Create a layout from this buffer's size and alignment.
    #[inline]
    pub fn layout(self) -> core::alloc::Layout {
        // SAFETY: align is always a power of two and <= size in valid allocations
        core::alloc::Layout::from_size_align(self.size, self.align).unwrap()
    }

    /// Get a non-owning pointer to the start of this buffer.
    #[inline]
    pub fn as_ptr(self) -> LpvmPtr {
        LpvmPtr::new(self.guest)
    }

    /// Get a non-owning pointer at the given byte offset from the start.
    ///
    /// Returns `None` if offset exceeds size.
    #[inline]
    pub fn offset_ptr(self, offset: usize) -> Option<LpvmPtr> {
        if offset > self.size {
            return None;
        }
        Some(LpvmPtr::new(self.guest + offset as u64))
    }

    /// Read bytes from this buffer at the given offset.
    ///
    /// # Safety
    /// The caller must ensure the buffer is valid and synchronized with guest access.
    pub unsafe fn read(&self, offset: usize, dst: &mut [u8]) -> Result<(), ()> {
        if offset.saturating_add(dst.len()) > self.size {
            return Err(());
        }
        unsafe {
            let src = self.native.add(offset);
            core::ptr::copy_nonoverlapping(src, dst.as_mut_ptr(), dst.len());
        }
        Ok(())
    }

    /// Write bytes to this buffer at the given offset.
    ///
    /// # Safety
    /// The caller must ensure the buffer is valid and synchronized with guest access.
    pub unsafe fn write(&self, offset: usize, src: &[u8]) -> Result<(), ()> {
        if offset.saturating_add(src.len()) > self.size {
            return Err(());
        }
        unsafe {
            let dst = self.native.add(offset);
            core::ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len());
        }
        Ok(())
    }
}

impl fmt::Debug for LpvmBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LpvmBuffer")
            .field("native", &self.native)
            .field("guest", &self.guest)
            .field("size", &self.size)
            .field("align", &self.align)
            .finish()
    }
}

/// Non-owning pointer into an [`LpvmBuffer`] or raw guest memory.
///
/// This is a lightweight, `Copy` type suitable for passing to shaders as uniforms
/// or for addressing within an existing allocation.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LpvmPtr {
    guest: u64,
}

impl LpvmPtr {
    /// Create a pointer with the given guest offset.
    #[inline]
    pub const fn new(guest: u64) -> Self {
        Self { guest }
    }

    /// Guest offset value (for uniforms).
    #[inline]
    pub fn guest_value(self) -> u64 {
        self.guest
    }

    /// Offset this pointer by the given number of bytes.
    #[inline]
    pub fn offset(self, bytes: usize) -> LpvmPtr {
        LpvmPtr::new(self.guest + bytes as u64)
    }
}

impl fmt::Debug for LpvmPtr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LpvmPtr")
            .field("guest", &self.guest)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn buffer_roundtrip() {
        let mut v = vec![0u8; 16];
        let buf = LpvmBuffer::new(v.as_mut_ptr(), 0x8000_0000, 16, 8);
        assert_eq!(buf.native_ptr(), v.as_mut_ptr());
        assert_eq!(buf.guest_base(), 0x8000_0000);
        assert_eq!(buf.size(), 16);
        assert_eq!(buf.align(), 8);
    }

    #[test]
    fn buffer_as_ptr() {
        let mut v = vec![0u8; 16];
        let buf = LpvmBuffer::new(v.as_mut_ptr(), 0x1000, 16, 8);
        let ptr = buf.as_ptr();
        assert_eq!(ptr.guest_value(), 0x1000);
    }

    #[test]
    fn buffer_offset_ptr() {
        let mut v = vec![0u8; 16];
        let buf = LpvmBuffer::new(v.as_mut_ptr(), 0x1000, 16, 8);
        assert!(buf.offset_ptr(0).is_some());
        assert!(buf.offset_ptr(16).is_some());
        assert!(buf.offset_ptr(17).is_none());
    }

    #[test]
    fn ptr_offset() {
        let p = LpvmPtr::new(0x1000);
        let q = p.offset(64);
        assert_eq!(q.guest_value(), 0x1040);
    }

    #[test]
    fn buffer_is_copy() {
        let mut v = vec![0u8; 4];
        let buf = LpvmBuffer::new(v.as_mut_ptr(), 0, 4, 4);
        let buf2 = buf;
        assert_eq!(buf.native_ptr(), buf2.native_ptr());
        assert_eq!(buf.guest_base(), buf2.guest_base());
    }

    #[test]
    fn ptr_is_copy() {
        let p = LpvmPtr::new(0x1000);
        let p2 = p;
        assert_eq!(p.guest_value(), p2.guest_value());
    }

    #[test]
    fn buffer_read_write() {
        let mut v = vec![0u8; 16];
        let buf = LpvmBuffer::new(v.as_mut_ptr(), 0, 16, 8);

        unsafe {
            buf.write(0, b"hello world!!!").unwrap();
            let mut dst = [0u8; 14];
            buf.read(0, &mut dst).unwrap();
            assert_eq!(&dst, b"hello world!!!");
        }
    }

    #[test]
    fn buffer_read_write_bounds() {
        let mut v = vec![0u8; 8];
        let buf = LpvmBuffer::new(v.as_mut_ptr(), 0, 8, 8);

        unsafe {
            assert!(buf.write(0, &[0u8; 9]).is_err());
            let mut dst = [0u8; 9];
            assert!(buf.read(0, &mut dst).is_err());
        }
    }
}
