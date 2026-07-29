//! Executable JIT code buffer (bytes live in RAM; on ESP32-C6 DRAM is executable).
//!
//! The buffer owns the *write* address of the emitted code — the `Vec<u8>` the
//! linker filled in and any patching writes through. [`JitBuffer::exec_ptr`] is
//! the single place that turns a write address into the address the CPU is
//! allowed to fetch from; see its docs for why those can differ.

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
    /// The one place in the crate where a *write* address becomes an *execute*
    /// address. The bytes were stored through the `Vec`'s own address; on some
    /// targets the instruction fetch path names those same bytes at a
    /// different address:
    ///
    /// - RV32 (ESP32-C6) and host: **identity**. DRAM is executable at the
    ///   address it is written at, so execute == write.
    /// - Xtensa (ESP32-S3), landing with the backport: **`exec = write +
    ///   0x6F_0000`**. Data writes go to the D-bus address; the CPU fetches
    ///   the same physical bytes through the I-bus alias 0x6F_0000 above it.
    ///   Hardware-proven (spike E2). That arm belongs here and nowhere else,
    ///   together with the belt-and-braces `memw` + `isync` after emission so
    ///   the fetch path observes the stores.
    ///
    /// Code **writers** (linking, relocation patching) must not use this: they
    /// operate on the buffer's own storage, which is already the write address.
    ///
    /// # Safety
    /// Same as dereferencing a function pointer into this buffer.
    #[cfg(not(target_arch = "xtensa"))]
    #[must_use]
    pub unsafe fn exec_ptr(&self, offset: usize) -> *const u8 {
        debug_assert!(offset <= self.code.len());
        debug_assert!(offset % 4 == 0);
        unsafe { self.code.as_ptr().add(offset) }
    }

    /// Xtensa sibling of [`exec_ptr`](Self::exec_ptr) — a per-board *rule*,
    /// not a bare constant:
    ///
    /// - **ESP32-S3**: writes inside SRAM1's dual-mapped window (D-bus
    ///   `0x3FC8_8000..0x3FCF_0000`, where the esp-alloc heap lives) execute
    ///   at the constant I-bus alias `+0x6F_0000`. Hardware-proven (spike E2;
    ///   no cache maintenance needed, `memw`+`isync` kept belt-and-braces by
    ///   the code writer).
    /// - **Classic ESP32** (M7 of the backport): the heap is NOT executable
    ///   and SRAM1's alias is word-mirrored — classic JIT buffers must be
    ///   IRAM-side allocations whose addresses are already executable, which
    ///   is the fall-through arm below. A heap write address outside a known
    ///   dual-mapped window is a placement bug, caught by the debug assert.
    ///
    /// # Safety
    /// Same as dereferencing a function pointer into this buffer.
    #[cfg(target_arch = "xtensa")]
    #[must_use]
    pub unsafe fn exec_ptr(&self, offset: usize) -> *const u8 {
        const S3_DUAL_MAPPED_DBUS: core::ops::Range<usize> = 0x3FC8_8000..0x3FCF_0000;
        const S3_IBUS_ALIAS_OFFSET: usize = 0x6F_0000;
        debug_assert!(offset <= self.code.len());
        debug_assert!(offset % 4 == 0);
        let write = unsafe { self.code.as_ptr().add(offset) };
        let addr = write as usize;
        if S3_DUAL_MAPPED_DBUS.contains(&addr) {
            (addr + S3_IBUS_ALIAS_OFFSET) as *const u8
        } else {
            debug_assert!(
                addr >= 0x4000_0000,
                "JIT buffer at {addr:#x} is neither in a known dual-mapped D-bus window nor an I-bus address"
            );
            write
        }
    }
}
