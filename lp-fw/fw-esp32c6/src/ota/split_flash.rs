//! Raw flash access for the update path: the region the boot records, the
//! core images and the engine live in, outside any filesystem.
//!
//! Two rules, both learned on silicon (XIAO C6, 2026-10-02):
//!
//! - **No second `FlashStorage`.** Its constructor sizes the part with an
//!   `RDID` on SPI1; made while the radios run, that came back as garbage,
//!   every bounds check then refused, and the core read its boot records as
//!   "missing". This uses esp-storage's low-level ROM calls instead, with the
//!   bounds below.
//! - **Writes are fenced.** Nothing is erased or written unless it lies in
//!   the region and outside the running core's own extent, or is one of the
//!   two boot-record sectors — and nothing at all until [`protect`] has been
//!   told where the running core is. The same bad read above computed the
//!   engine's room as starting on top of the running core; the fence turns
//!   that class of bug into a refused update instead of a self-erasing board.
//!
//! [`protect`]: SplitFlash::protect

use lp_bootctl::{BOOT_RECORD_SECTORS, Extent, REGION_END_C6_4MB, REGION_START};

pub const SECTOR: u32 = 4096;

/// A word-aligned 4 KiB buffer: the ROM routines take word-aligned buffers.
#[repr(C, align(4))]
pub struct SectorBuf(pub [u8; SECTOR as usize]);

/// The SPI flash, for the update path only.
pub struct SplitFlash {
    /// The running core's extent; `None` = no writes at all.
    core: Option<Extent>,
    unlocked: bool,
}

impl SplitFlash {
    pub fn take() -> Self {
        Self {
            core: None,
            unlocked: false,
        }
    }

    /// Allow writes, never inside `core` (the running core's extent).
    pub fn protect(&mut self, core: Extent) {
        self.core = Some(core);
    }

    /// Read `out.len()` bytes at `at` (both multiples of 4).
    pub fn read(&mut self, at: u32, out: &mut [u8]) -> bool {
        #[repr(C, align(4))]
        struct Chunk([u8; 64]);
        let mut chunk = Chunk([0; 64]);
        let mut done = 0;
        while done < out.len() {
            let n = (out.len() - done).min(64);
            let len = (n + 3) & !3;
            // SAFETY: the ROM read into a local word-aligned buffer.
            let ok = unsafe {
                esp_storage::ll::spiflash_read(
                    at + done as u32,
                    chunk.0.as_mut_ptr().cast(),
                    len as u32,
                )
            }
            .is_ok();
            if !ok {
                return false;
            }
            out[done..done + n].copy_from_slice(&chunk.0[..n]);
            done += n;
        }
        true
    }

    fn allowed(&self, at: u32, len: u32) -> bool {
        let Some(core) = self.core else {
            return false;
        };
        if BOOT_RECORD_SECTORS
            .iter()
            .any(|s| at >= *s && at + len <= s + SECTOR)
        {
            return true;
        }
        let in_region = at >= REGION_START && at + len <= REGION_END_C6_4MB;
        let clear_of_core = at + len <= core.start || at >= core.end;
        in_region && clear_of_core
    }

    fn unlock(&mut self) -> bool {
        if !self.unlocked {
            // SAFETY: the ROM's write-protect clear.
            self.unlocked = unsafe { esp_storage::ll::spiflash_unlock() }.is_ok();
        }
        self.unlocked
    }

    pub fn erase(&mut self, at: u32) -> bool {
        if at % SECTOR != 0 || !self.allowed(at, SECTOR) {
            super::say!("[OTA] refused: erase {at:#x} is outside what may be written");
            return false;
        }
        // SAFETY: a sector the fence allows.
        let ok =
            self.unlock() && unsafe { esp_storage::ll::spiflash_erase_sector(at / SECTOR) }.is_ok();
        if !ok {
            log::error!("[OTA] erase {at:#x} failed");
        }
        ok
    }

    /// Erase the sector at `at` and write `data` (≤ 4 KiB) at its start.
    pub fn write_sector(&mut self, buf: &mut SectorBuf, at: u32, data: &[u8]) -> bool {
        if !self.erase(at) {
            return false;
        }
        buf.0.fill(0xff);
        buf.0[..data.len()].copy_from_slice(data);
        let len = ((data.len() + 3) & !3) as u32;
        // SAFETY: an erased sector the fence allows; a word-aligned buffer.
        let ok = unsafe { esp_storage::ll::spiflash_write(at, buf.0.as_ptr().cast(), len) }.is_ok();
        if !ok {
            log::error!("[OTA] write {at:#x} failed");
        }
        ok
    }

    /// Program one word in place, without an erase: only 1 → 0 bits change.
    pub fn program_word(&mut self, at: u32, word: u32) -> bool {
        if !self.allowed(at, 4) {
            log::error!("[OTA] refused: program {at:#x}");
            return false;
        }
        let w = word;
        // SAFETY: one word the fence allows.
        self.unlock() && unsafe { esp_storage::ll::spiflash_write(at, &w, 4) }.is_ok()
    }
}
