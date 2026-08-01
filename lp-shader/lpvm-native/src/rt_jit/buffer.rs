//! Executable JIT code buffer.
//!
//! Two placements exist:
//!
//! - **Heap** (host, ESP32-C6, ESP32-S3): the buffer owns the *write* address
//!   of the emitted code — the `Vec<u8>` the linker filled in and any patching
//!   writes through. [`JitBuffer::exec_ptr`] turns that into the address the
//!   CPU is allowed to fetch from, applying the rule in [`crate::exec_addr`];
//!   see there for why the two can differ.
//! - **Placed** (classic ESP32): the code was *copied out* of its staging Vec
//!   into a fixed executable region ([`crate::codemem_esp32`]) because the
//!   heap has no I-bus view at all. The buffer holds the installed image's
//!   I-bus base directly; [`crate::exec_addr`]'s in-place rule is never
//!   consulted. The span belongs to the [`crate::codemem_esp32::CodeArena`]
//!   it came from — dropping the buffer does NOT free it; the engine that
//!   allocated the span owns that lifetime.

use alloc::vec::Vec;

enum Inner {
    /// Emitted code executing in place out of its own allocation.
    Heap { code: Vec<u8> },
    /// Code installed at a fixed executable address; `exec_base` is already
    /// an execute (I-bus) address. The span belongs to an EXTERNAL arena the
    /// caller owns — dropping the buffer does not free it.
    Placed { exec_base: usize, len: usize },
    /// As `Placed`, but the span came from `codemem_esp32::global`'s arena
    /// (the `xt-placed-code` firmware path) and is returned to it on drop —
    /// without this, every Studio recompile would leak a slice of the 92 KiB
    /// region until `TooLarge`.
    #[cfg(feature = "xt-placed-code")]
    PlacedGlobal { exec_base: usize, len: usize },
}

#[cfg(feature = "xt-placed-code")]
impl Drop for JitBuffer {
    fn drop(&mut self) {
        if let Inner::PlacedGlobal { exec_base, len } = self.inner {
            // NotInstalled/Busy cannot free the span; leaking is the safe
            // failure (a leak is recoverable by reboot, a double-use is
            // corruption).
            let _ = crate::codemem_esp32::global::with(|arena| {
                arena.free(exec_base as u32, len as u32);
            });
        }
    }
}

/// Holds emitted machine code for one module.
pub struct JitBuffer {
    inner: Inner,
}

impl JitBuffer {
    pub(crate) fn from_code(code: Vec<u8>) -> Self {
        Self {
            inner: Inner::Heap { code },
        }
    }

    /// A buffer for code already installed at execute address `exec_base`
    /// (classic ESP32 fixed-region path). The caller guarantees `len` bytes
    /// of linked code are installed there and synced before the first call.
    pub(crate) fn from_placed(exec_base: usize, len: usize) -> Self {
        debug_assert!(exec_base % 4 == 0);
        Self {
            inner: Inner::Placed { exec_base, len },
        }
    }

    /// [`JitBuffer::from_placed`] for a span owned by the global arena; the
    /// span is freed back to it when this buffer drops.
    #[cfg(feature = "xt-placed-code")]
    pub(crate) fn from_placed_global(exec_base: usize, len: usize) -> Self {
        debug_assert!(exec_base % 4 == 0);
        Self {
            inner: Inner::PlacedGlobal { exec_base, len },
        }
    }

    /// Byte length of emitted code.
    #[must_use]
    pub fn len(&self) -> usize {
        match &self.inner {
            Inner::Heap { code } => code.len(),
            Inner::Placed { len, .. } => *len,
            #[cfg(feature = "xt-placed-code")]
            Inner::PlacedGlobal { len, .. } => *len,
        }
    }

    /// True if no code was emitted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// **Execute** address of the code at byte `offset` (must be 4-byte
    /// aligned, in bounds).
    ///
    /// Heap placement: the bytes were stored through the `Vec`'s own address;
    /// on some targets the instruction fetch path names those same bytes at a
    /// different one. That rule lives in [`crate::exec_addr`] — this is one
    /// of its two callers, the other being the linker, which patches
    /// intra-module call targets into the image. (No explicit barrier is
    /// needed on either Xtensa chip: internal SRAM is uncached, measured on
    /// silicon — the placed path's sink still fences as free insurance.)
    ///
    /// Placed placement: `exec_base` is already an execute address, so this
    /// is plain offset arithmetic.
    ///
    /// Code **writers** (linking, relocation patching) address the staging
    /// buffer's own storage, which is already the write address — they must
    /// not route *that* through here. The addresses they store *into* the
    /// code are a different matter: those are jump targets, and they do need
    /// the placement's rule (`link_jit` in place, `link_jit_at` for placed
    /// images).
    ///
    /// # Safety
    /// Same as dereferencing a function pointer into this buffer.
    #[must_use]
    pub unsafe fn exec_ptr(&self, offset: usize) -> *const u8 {
        debug_assert!(offset <= self.len());
        debug_assert!(offset % 4 == 0);
        match &self.inner {
            Inner::Heap { code } => {
                let write = unsafe { code.as_ptr().add(offset) };
                crate::exec_addr::exec_addr(write as usize) as *const u8
            }
            Inner::Placed { exec_base, .. } => (exec_base + offset) as *const u8,
            #[cfg(feature = "xt-placed-code")]
            Inner::PlacedGlobal { exec_base, .. } => (exec_base + offset) as *const u8,
        }
    }
}
