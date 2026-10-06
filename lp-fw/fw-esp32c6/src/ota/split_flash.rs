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
//!   boot-record sectors or the update's progress-record sector — and
//!   nothing at all until [`protect`] has been told
//!   where the running core is and where the region ends (the end the
//!   flashed partition table gives). The same bad read above computed the
//!   engine's room as starting on top of the running core; the fence turns
//!   that class of bug into a refused write instead of a self-erasing board.
//!
//! [`protect`]: SplitFlash::protect

use lp_bootctl::{BOOT_RECORD_SECTORS, Extent, PROGRESS_RECORD_SECTOR, REGION_START};

pub const SECTOR: u32 = 4096;

/// The bytes the ROM moves per call: a word-aligned local buffer, small
/// enough for the stack and never across a 256-byte program page.
const PIECE: usize = 64;

#[repr(C, align(4))]
struct Piece([u8; PIECE]);

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

    /// Read `out.len()` bytes at `at`, any alignment.
    pub fn read(&mut self, at: u32, out: &mut [u8]) -> bool {
        let mut chunk = Piece([0; PIECE]);
        let mut done = 0;
        while done < out.len() {
            let addr = at + done as u32;
            let base = addr & !3;
            let skip = (addr - base) as usize;
            let n = (out.len() - done).min(PIECE - skip);
            let len = (skip + n + 3) & !3;
            // SAFETY: the ROM read, word-aligned, into a local word-aligned
            // buffer.
            let ok = unsafe {
                esp_storage::ll::spiflash_read(base, chunk.0.as_mut_ptr().cast(), len as u32)
            }
            .is_ok();
            if !ok {
                return false;
            }
            out[done..done + n].copy_from_slice(&chunk.0[skip..skip + n]);
            done += n;
        }
        true
    }

    /// Erase the 4 KiB sector at `at` (sector-aligned), if the fence allows.
    pub fn erase(&mut self, at: u32) -> bool {
        if at % SECTOR != 0 || !self.allowed(at, SECTOR) {
            log::error!("[OTA] refused: erase {at:#x}");
            return false;
        }
        // SAFETY: one sector the fence allows.
        self.unlock() && unsafe { esp_storage::ll::spiflash_erase_sector(at / SECTOR) }.is_ok()
    }

    /// Program `bytes` at `at`, any alignment: NOR only clears bits, so the
    /// word-aligned span around them is written with `0xFF` where nothing is
    /// to change.
    pub fn program(&mut self, at: u32, bytes: &[u8]) -> bool {
        if !self.allowed(at, bytes.len() as u32) {
            log::error!("[OTA] refused: program {at:#x} +{}", bytes.len());
            return false;
        }
        if !self.unlock() {
            return false;
        }
        let mut chunk = Piece([0xff; PIECE]);
        let mut done = 0;
        while done < bytes.len() {
            let addr = at + done as u32;
            let base = addr & !3;
            let skip = (addr - base) as usize;
            let n = (bytes.len() - done).min(PIECE - skip);
            let len = (skip + n + 3) & !3;
            chunk.0.fill(0xff);
            chunk.0[skip..skip + n].copy_from_slice(&bytes[done..done + n]);
            // SAFETY: a span the fence allows (its padding is 0xFF, which
            // programs nothing), from a local word-aligned buffer.
            let ok = unsafe {
                esp_storage::ll::spiflash_write(base, chunk.0.as_ptr().cast(), len as u32)
            }
            .is_ok();
            if !ok {
                return false;
            }
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
            .chain(core::iter::once(&PROGRESS_RECORD_SECTOR))
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
