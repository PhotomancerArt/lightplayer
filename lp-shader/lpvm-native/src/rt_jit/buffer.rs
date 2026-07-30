//! Executable JIT code buffer (bytes live in RAM; on ESP32-C6 DRAM is executable).
//!
//! The buffer owns the *write* address of the emitted code — the `Vec<u8>` the
//! linker filled in and any patching writes through. [`JitBuffer::exec_ptr`]
//! turns that into the address the CPU is allowed to fetch from, applying the
//! rule in [`crate::exec_addr`]; see there for why the two can differ.

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

    /// **Execute** address of the code at byte `offset` (must be 4-byte
    /// aligned, in bounds).
    ///
    /// The bytes were stored through the `Vec`'s own address; on some targets
    /// the instruction fetch path names those same bytes at a different one.
    /// That rule lives in [`crate::exec_addr`] — this is one of its two
    /// callers, the other being the linker, which patches intra-module call
    /// targets into the image.
    ///
    /// On Xtensa the rule is paired with the belt-and-braces `memw` + `isync`
    /// after emission, so the fetch path observes the stores.
    ///
    /// Code **writers** (linking, relocation patching) address the buffer's
    /// own storage, which is already the write address — they must not route
    /// *that* through here. The addresses they store *into* the code are a
    /// different matter: those are jump targets, and they do need the rule.
    ///
    /// # Safety
    /// Same as dereferencing a function pointer into this buffer.
    #[must_use]
    pub unsafe fn exec_ptr(&self, offset: usize) -> *const u8 {
        debug_assert!(offset <= self.code.len());
        debug_assert!(offset % 4 == 0);
        let write = unsafe { self.code.as_ptr().add(offset) };
        crate::exec_addr::exec_addr(write as usize) as *const u8
    }
}
