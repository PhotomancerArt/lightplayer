//! Host/guest pointer pair for shared LPVM memory.

use core::fmt;

/// Pointer to shared memory visible to both host and guest code.
///
/// The host uses [`Self::native_ptr`] for direct access (unsafe). The guest
/// receives [`Self::guest_value`] through uniforms and uses it in Load/Store.
///
/// # Guest value width
///
/// `guest_value` is always `u64`. On 32-bit targets (WASM linear memory,
/// RV32, ESP32), only the low 32 bits are used. On 64-bit JIT hosts, the
/// full value may be a zero-extended or full pointer, depending on the backend.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ShaderPtr {
    native: *mut u8,
    guest: u64,
}

impl ShaderPtr {
    /// Build a pointer pair. Creating this value is safe; dereferencing
    /// `native` is only safe when the allocation is valid and synchronized.
    #[inline]
    pub const fn new(native: *mut u8, guest: u64) -> Self {
        Self { native, guest }
    }

    /// Host pointer into the shared allocation.
    ///
    /// # Safety
    ///
    /// Dereferencing requires a valid allocation, correct bounds, and
    /// synchronization with concurrent guest access.
    #[inline]
    pub fn native_ptr(self) -> *mut u8 {
        self.native
    }

    /// Value passed to the guest (e.g. as a uniform).
    #[inline]
    pub fn guest_value(self) -> u64 {
        self.guest
    }
}

impl fmt::Debug for ShaderPtr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShaderPtr")
            .field("native", &self.native)
            .field("guest", &self.guest)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn shader_ptr_roundtrip() {
        let mut v = vec![0u8; 16];
        let native = v.as_mut_ptr();
        let guest: u64 = 0x8000_0000;
        let ptr = ShaderPtr::new(native, guest);
        assert_eq!(ptr.native_ptr(), native);
        assert_eq!(ptr.guest_value(), guest);
    }

    #[test]
    fn shader_ptr_is_copy() {
        let mut v = vec![0u8; 4];
        let ptr = ShaderPtr::new(v.as_mut_ptr(), 0);
        let ptr2 = ptr;
        assert_eq!(ptr.native_ptr(), ptr2.native_ptr());
        assert_eq!(ptr.guest_value(), ptr2.guest_value());
    }
}
