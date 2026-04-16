//! Executable JIT code buffer (bytes live in RAM; on ESP32-C6 DRAM is executable).

use alloc::vec::Vec;

/// Holds emitted RISC-V machine code for one module.
pub struct JitBuffer {
    code: Vec<u8>,
}

impl JitBuffer {
    pub(crate) fn from_code(code: Vec<u8>) -> Self {
        Self { code }
    }

    /// Byte length of emitted code.
    #[must_use]
    pub fn len(&self) -> usize {
        self.code.len()
    }

    /// True if no code was emitted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.code.is_empty()
    }

    /// Entry address at byte offset (must be 4-byte aligned, in bounds).
    ///
    /// # Safety
    /// Same as dereferencing a function pointer into this buffer.
    #[must_use]
    pub unsafe fn entry_ptr(&self, offset: usize) -> *const u8 {
        debug_assert!(offset <= self.code.len());
        debug_assert!(offset % 4 == 0);
        unsafe { self.code.as_ptr().add(offset) }
    }
}
