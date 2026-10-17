//! The tree store's flash on the C6 (`fs-tree`): `lpfs` from the flashed
//! partition table, written through the word-alignment shim
//! (`fw_esp32_common::aligned_nor_flash`, plan D10) over esp-storage, and
//! **read through the flash cache** (the mount study's C7).
//!
//! **Writes.** esp-storage on the C6 is built with `panic-unaligned-buffer`
//! and without `bytewise-read`: an offset or length that is not a multiple
//! of 4 is `NotAligned`, and a buffer at an address that is not one
//! **panics**. The store writes records at any byte offset, so every program
//! goes through the shim's aligned bounce buffer and reaches the ROM
//! word-aligned and padded with `0xFF` (M4's owed "unaligned program"
//! sitting: the store's unaligned records never reach the ROM as such).
//! Programs go through `NorFlash::write`, never `Storage::write`
//! (read-erase-write of a whole sector).
//!
//! **Reads.** A ROM read call (SPI1) costs ~14 µs on CX1 whatever it reads
//! (#1154's fit), and mount and the boot's file reads are thousands of small
//! ones. So the partition is mapped by the MMU ([`LpfsWindow`]) and the store
//! reads it as memory: a 32-byte line fill (~2.1 µs, measured on silicon)
//! the first time a line is touched, nothing after. The window takes the
//! **top** of the loader's old scratch window (`0x4260_0000..0x4280_0000`,
//! unused once the core runs): the core is linked below `0x4240_0000` and
//! the engine from it, and the two together never exceed the app partition
//! (3.25 MB), which ends below the window's first page — no MMU entry here is
//! one either of them uses. The core's own hash maps its scratch pages
//! just below the engine (`ota/engine_window.rs`): disjoint.
//!
//! **Coherence.** The cache knows nothing of SPI1's programs and erases.
//! `lp_tree_store::WindowedFlash` invalidates the range after every one,
//! before the store reads again — the mask ROM's `Cache_Invalidate_Addr`,
//! by line, never the whole cache (that would throw away the running code:
//! ~1,024 line fills). Nothing else reads or writes `lpfs` while the store
//! owns it, and no ISR touches it.

use esp_storage::FlashStorage;
use fw_esp32_common::aligned_nor_flash::AlignedNorFlash;
use lp_tree_store::{ReadWindow, WindowedFlash};

use crate::flash_layout::PartitionExtent;

/// The store's sector: the C6 flash's erase sector.
const SECTOR_SIZE: u32 = 4096;

/// The partition through the alignment shim (the writes, and the reads when
/// no window could be mapped).
pub type TreePart = AlignedNorFlash<FlashStorage<'static>>;

/// `lpfs` as the tree store sees it.
pub type TreeFlash = WindowedFlash<TreePart, Option<LpfsWindow>>;

/// The store's flash over the located `lpfs` partition. Records are checked
/// by the mask ROM's table-driven `crc32_le` (FORMAT.md's CRC-32/ISO-HDLC,
/// the same values as `lp-crc32`): ~8x fewer cycles a byte than the bitwise
/// one, which bounds mount and every record read, for no flash.
pub fn tree_flash(flash: FlashStorage<'static>, partition: PartitionExtent) -> TreeFlash {
    let part = AlignedNorFlash::new(flash, partition.offset, partition.len, SECTOR_SIZE)
        .with_crc32(esp_hal::rom::crc::crc32_le);
    let window = LpfsWindow::map(partition.offset, partition.len);
    if window.is_none() {
        esp_println::println!("[FS] lpfs is read by ROM calls: it does not fit its cache window");
    }
    WindowedFlash::new(part, window)
}

/// Is a pre-repartition LightPlayer filesystem waiting at the legacy offset
/// (probed read-only; Q5: kept, so littlefs stays linked)? Asked only when
/// the store found no store on `lpfs`.
pub fn legacy_lpfs_present(flash: &mut TreeFlash) -> bool {
    // The size measurement's build (`measure_no_legacy_probe`, never
    // shipped): no probe, so LTO drops littlefs.
    #[cfg(feature = "measure_no_legacy_probe")]
    {
        let _ = flash;
        false
    }
    #[cfg(not(feature = "measure_no_legacy_probe"))]
    {
        let part = flash.flash_mut();
        let lpfs_offset = part.offset();
        crate::flash_storage::legacy_lpfs_present_at(part.part_mut(), lpfs_offset)
    }
}

const SPI0: usize = 0x6000_2000;
const MMU_ITEM_CONTENT: usize = SPI0 + 0x37c;
const MMU_ITEM_INDEX: usize = SPI0 + 0x380;
const MMU_POWER_CTRL: usize = SPI0 + 0x384;
const MMU_VALID: u32 = 1 << 9;
/// The cached flash window's start (MMU entry 0).
const CACHE_BASE: usize = 0x4200_0000;
/// The loader's old scratch window (`fw-esp32c6-loader/src/flash_window.rs`).
const SCRATCH: core::ops::Range<usize> = 0x4260_0000..0x4280_0000;
/// The cache's line.
const LINE: usize = 32;

unsafe extern "C" {
    /// The mask ROM's range invalidate (`esp32c6.rom.ld`): every cache line
    /// holding a byte of `[vaddr, vaddr + size)` is dropped. 0 = done.
    fn Cache_Invalidate_Addr(vaddr: u32, size: u32) -> i32;
}

/// `lpfs` mapped into the flash cache: the store reads it as memory.
pub struct LpfsWindow {
    /// The partition's first byte, as a cached address.
    base: usize,
    len: usize,
    /// An invalidate the ROM refused: the window may hold stale lines, so
    /// every read from then on goes to the flash (ROM calls), as with no
    /// window at all.
    stale: bool,
}

impl LpfsWindow {
    /// Map `len` bytes of flash from `offset` to the top of the scratch
    /// window, one MMU entry per page; `None` when they do not fit there.
    fn map(offset: u32, len: u32) -> Option<Self> {
        // SAFETY: a read of an SPI0 register.
        let mode = (unsafe { core::ptr::read_volatile(MMU_POWER_CTRL as *const u32) } >> 3) & 3;
        let shift = 16 - mode;
        let page = 1u32 << shift;
        let first = offset >> shift;
        let pages = (offset + len).div_ceil(page) - first;
        let vbase = SCRATCH.end.checked_sub((pages as usize) << shift)?;
        if vbase < SCRATCH.start {
            return None;
        }
        let entry0 = ((vbase - CACHE_BASE) >> shift) as u32;
        for k in 0..pages {
            // SAFETY: SPI0's MMU registers; entries of the scratch window's
            // top, which nothing running uses (module docs).
            unsafe {
                core::ptr::write_volatile(MMU_ITEM_INDEX as *mut u32, entry0 + k);
                core::ptr::write_volatile(MMU_ITEM_CONTENT as *mut u32, (first + k) | MMU_VALID);
            }
        }
        let base = vbase + (offset - (first << shift)) as usize;
        // Whatever the cache held at these addresses (the loader read the
        // core through them) is another page's bytes now.
        if !invalidate(base, len as usize) {
            return None;
        }
        Some(Self {
            base,
            len: len as usize,
            stale: false,
        })
    }
}

impl ReadWindow<TreePart> for LpfsWindow {
    fn read(
        &mut self,
        flash: &mut TreePart,
        addr: u32,
        buf: &mut [u8],
    ) -> Result<(), esp_storage::FlashStorageError> {
        let at = addr as usize;
        if self.stale || at + buf.len() > self.len {
            return lp_tree_store::Flash::read(flash, addr, buf);
        }
        // SAFETY: `[base, base + len)` is the mapped partition (`map`), and
        // `at + buf.len()` lies inside it.
        unsafe {
            core::ptr::copy_nonoverlapping(
                (self.base + at) as *const u8,
                buf.as_mut_ptr(),
                buf.len(),
            );
        }
        Ok(())
    }

    fn invalidate(&mut self, addr: u32, len: u32) {
        if !invalidate(self.base + addr as usize, len as usize) {
            self.stale = true;
        }
    }
}

/// Drop every cache line holding a byte of `[vaddr, vaddr + len)`; `false`
/// when the ROM refused the range (it checks both ends are cacheable).
fn invalidate(vaddr: usize, len: usize) -> bool {
    let start = vaddr & !(LINE - 1);
    let end = (vaddr + len).next_multiple_of(LINE);
    // A barrier for the compiler: no load of the window may move across it.
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
    // SAFETY: a ROM routine over addresses of the mapped window.
    let refused = unsafe { Cache_Invalidate_Addr(start as u32, (end - start) as u32) } != 0;
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
    !refused
}
