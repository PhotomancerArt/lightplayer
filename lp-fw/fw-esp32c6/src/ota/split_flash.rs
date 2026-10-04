//! Raw flash access for the boot bookkeeping: the region the boot records,
//! the core images and the engine live in, outside any filesystem.
//!
//! Two rules, both learned on silicon (XIAO C6, 2026-10-02):
//!
//! - **No second `FlashStorage`.** Its constructor sizes the part with an
//!   `RDID` on SPI1; made while the radios run, that came back as garbage,
//!   every bounds check then refused, and the core read its boot records as
//!   "missing". This uses esp-storage's low-level ROM calls instead, with the
//!   bounds below.
//! - **Writes are fenced.** Nothing is written unless it lies in the region
//!   and outside the running core's own extent, or is one of the two
//!   boot-record sectors — and nothing at all until [`protect`] has been told
//!   where the running core is and where the region ends (the end the
//!   flashed partition table gives). The same bad read above computed the
//!   engine's room as starting on top of the running core; the fence turns
//!   that class of bug into a refused write instead of a self-erasing board.
//!
//! [`protect`]: SplitFlash::protect

use lp_bootctl::{BOOT_RECORD_SECTORS, Extent, REGION_START};

const SECTOR: u32 = 4096;

/// The SPI flash, for the boot bookkeeping only.
pub struct SplitFlash {
    /// The running core's extent and the region's end; `None` = no writes
    /// at all.
    fence: Option<(Extent, u32)>,
    unlocked: bool,
}

impl SplitFlash {
    pub fn take() -> Self {
        Self {
            fence: None,
            unlocked: false,
        }
    }

    /// Allow writes inside the region ending at `region_end`, never inside
    /// `core` (the running core's extent).
    pub fn protect(&mut self, core: Extent, region_end: u32) {
        self.fence = Some((core, region_end));
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
        let Some((core, region_end)) = self.fence else {
            return false;
        };
        if BOOT_RECORD_SECTORS
            .iter()
            .any(|s| at >= *s && at + len <= s + SECTOR)
        {
            return true;
        }
        let in_region = at >= REGION_START && at + len <= region_end;
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
