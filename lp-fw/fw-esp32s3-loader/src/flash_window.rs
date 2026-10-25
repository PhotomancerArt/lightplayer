//! Reading flash through the cache, at the loader's scratch window.
//!
//! The loader never uses the ROM's SPI1 routines (`esp_rom_spiflash_read`):
//! on the C6 they left SPI1 in a state where the core's flash driver then
//! failed (found on silicon, 2026-10-02), and the S3 loader keeps the rule.
//! Reading through the cache leaves SPI1 exactly as the bootloader left it.
//!
//! The window is the MMU entries the loader claims for itself
//! (`lp_bootctl::s3_mmu::LOADER_SCRATCH`, `0x1C0..0x200`, 4 MiB at
//! `0x3DC0_0000` on the data bus): clear of the core's entries (from 0) and
//! the engine's (from `0x100`), and unmapped before the jump.

use lp_bootctl::s3_mmu::{LOADER_SCRATCH, LOADER_SCRATCH_DROM, MMU_PAGE};

use crate::{mmu, rom};

/// How much flash the window holds: 4 MiB.
pub const SCRATCH_LEN: u32 = (LOADER_SCRATCH.end - LOADER_SCRATCH.start) * MMU_PAGE;

/// A run of flash mapped for reading.
pub struct FlashWindow {
    /// The flash offset of the first mapped page.
    base: u32,
    len: u32,
}

impl FlashWindow {
    /// Map `[at, at + len)` (rounded out to pages) at the scratch window.
    pub fn map(at: u32, len: u32) -> Option<Self> {
        let base = at - at % MMU_PAGE;
        let mapped = at.checked_add(len)?.checked_sub(base)?.div_ceil(MMU_PAGE) * MMU_PAGE;
        if mapped > SCRATCH_LEN {
            return None;
        }
        for k in 0..mapped / MMU_PAGE {
            mmu::map(LOADER_SCRATCH.start + k, base + k * MMU_PAGE);
        }
        rom::invalidate_caches();
        Some(Self { base, len: mapped })
    }

    /// Whether `[at, at + len)` is inside the window.
    pub fn has(&self, at: u32, len: u32) -> bool {
        at >= self.base
            && at
                .checked_add(len)
                .is_some_and(|end| end <= self.base + self.len)
    }

    /// Where flash offset `at` (inside the window) reads on the data bus.
    pub fn addr(&self, at: u32) -> *const u8 {
        (LOADER_SCRATCH_DROM + (at - self.base)) as *const u8
    }

    /// Copy `len` bytes of flash at `at` (inside the window) to `dest`, a
    /// byte-addressable RAM: words while everything is aligned, bytes after.
    ///
    /// # Safety
    /// `dest..dest + len` must be writable and not overlap the loader.
    pub unsafe fn copy(&self, at: u32, dest: *mut u8, len: u32) -> bool {
        if !self.has(at, len) {
            return false;
        }
        let src = self.addr(at);
        let words = if (src as usize | dest as usize).is_multiple_of(4) {
            len as usize / 4
        } else {
            0
        };
        // SAFETY: `src..src+len` is mapped flash; the caller owns `dest`.
        unsafe {
            copy_words(src as *const u32, dest as *mut u32, words);
            for i in words * 4..len as usize {
                dest.add(i).write_volatile(src.add(i).read_volatile());
            }
        }
        true
    }

    /// Copy `len` bytes (a multiple of 4) of flash at `at` to `dest` with
    /// 32-bit stores only: what instruction RAM without a data-bus alias
    /// takes.
    ///
    /// # Safety
    /// As [`FlashWindow::copy`]; `dest` and `len` 4-aligned.
    pub unsafe fn copy_words(&self, at: u32, dest: *mut u32, len: u32) -> bool {
        if !self.has(at, len) || !at.is_multiple_of(4) || !len.is_multiple_of(4) {
            return false;
        }
        // SAFETY: as above, word-aligned.
        unsafe { copy_words(self.addr(at) as *const u32, dest, len as usize / 4) };
        true
    }

    /// A little-endian word of flash at `at`.
    pub fn word(&self, at: u32) -> Option<u32> {
        let mut w = [0u8; 4];
        // SAFETY: a local buffer.
        unsafe { self.copy(at, w.as_mut_ptr(), 4) }.then(|| u32::from_le_bytes(w))
    }
}

/// # Safety
/// Both runs valid for `words` aligned words.
unsafe fn copy_words(src: *const u32, dest: *mut u32, words: usize) {
    for i in 0..words {
        // SAFETY: the caller's contract.
        unsafe { dest.add(i).write_volatile(src.add(i).read_volatile()) };
    }
}
