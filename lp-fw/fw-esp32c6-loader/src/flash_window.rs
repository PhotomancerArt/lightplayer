//! Reading flash through the cache, at a scratch window.
//!
//! The loader deliberately does NOT use the ROM's SPI1 routines
//! (`esp_rom_spiflash_read`): on silicon they leave SPI1 in a state where
//! the core's flash driver then fails (esp-storage sizes the part with an
//! `RDID` on SPI1 and got garbage, so every bounds check refused — found on a
//! XIAO C6, 2026-10-02; the emulator models neither). Reading through SPI0
//! and the cache leaves SPI1 exactly as the bootloader left it.
//!
//! The scratch window sits in the top of the 8 MiB the IDF bootloader's MMU
//! covers, clear of the core (from `0x4200_0000`) and of the engine the core
//! maps later (from `0x4240_0000`, under 2 MiB).

use crate::mmu;

/// Where flash is mapped for reading.
const SCRATCH: u32 = 0x4260_0000;
/// How much of it there is.
const SCRATCH_LEN: u32 = 0x0020_0000;

/// A run of flash mapped for reading; `read` copies out of it.
pub struct FlashWindow {
    /// The flash offset of the first mapped page.
    base: u32,
    len: u32,
}

impl FlashWindow {
    /// Map `[at, at + len)` (rounded out to pages) at the scratch window.
    pub fn map(at: u32, len: u32, page_shift: u32) -> Option<Self> {
        let page = 1u32 << page_shift;
        let base = at - at % page;
        let mapped = (at + len - base).div_ceil(page) * page;
        if mapped > SCRATCH_LEN {
            return None;
        }
        for k in 0..mapped / page {
            mmu::map(SCRATCH + k * page, base + k * page, page_shift);
        }
        crate::rom::invalidate_cache();
        Some(Self { base, len: mapped })
    }

    /// Copy `len` bytes of flash at `at` (inside the window) to `dest`.
    ///
    /// # Safety
    /// `dest..dest + len` must be writable and not overlap the loader.
    pub unsafe fn copy(&self, at: u32, dest: *mut u8, len: u32) -> bool {
        if at < self.base || at + len > self.base + self.len {
            return false;
        }
        let src = (SCRATCH + (at - self.base)) as *const u8;
        // SAFETY: `src..src+len` is mapped flash; the caller owns `dest`.
        for i in 0..len as usize {
            unsafe { dest.add(i).write_volatile(src.add(i).read_volatile()) };
        }
        true
    }

    /// A little-endian word of flash at `at`.
    pub fn word(&self, at: u32) -> Option<u32> {
        let mut w = [0u8; 4];
        // SAFETY: a local buffer.
        unsafe { self.copy(at, w.as_mut_ptr(), 4) }.then(|| u32::from_le_bytes(w))
    }
}
