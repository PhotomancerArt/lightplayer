//! The tree store's flash on the S3 — the C6's `tree_flash.rs` on this
//! chip: `lpfs` from the flashed partition table (1.5 MB at `0x610000`, 384
//! sectors), written through the word-alignment shim
//! (`fw_esp32_common::aligned_nor_flash`) over esp-storage, and **read
//! through the flash cache**.
//!
//! **Writes.** esp-storage here is built with `panic-unaligned-buffer` and
//! without `bytewise-read`, as on the C6: an offset or length that is not a
//! multiple of 4 is `NotAligned`, and a buffer at an address that is not one
//! panics. The store writes records at any byte offset, so every program goes
//! through the shim's aligned bounce buffer. Programs go through
//! `NorFlash::write`, never `Storage::write` (read-erase-write of a sector).
//!
//! **Reads.** The partition is mapped by the MMU ([`LpfsWindow`]) and the
//! store reads it as memory. The window is layout 2's `lpfs` window
//! (`lp_bootctl::s3_mmu::LPFS_WINDOW`, entries `0x1C0..0x1E0`, data view
//! `0x3DC0_0000`, up to 2 MiB): the loader's scratch entries, which the
//! loader unmaps before it jumps and a plain image never maps. The core is
//! linked below entry `0x100`, the engine from it, and the core's own scratch
//! is `0x1E0..0x200`: no entry here is one any of them uses.
//!
//! **Coherence.** The cache knows nothing of SPI1's programs and erases.
//! `lp_tree_store::WindowedFlash` invalidates the range after every one,
//! before the store reads again — the S3 mask ROM's `Cache_Invalidate_Addr`,
//! by range, never the whole cache.
//!
//! **littlefs, by its magic only.** littlefs left the S3 with the tree store
//! (the S3 updates plan's P9; the C6's cutover, AD5): nothing here mounts it.
//! A board whose files are still in littlefs is recognised by the
//! superblock's name ([`littlefs_found`]) and HELD — never formatted. No S3
//! is in the field but the bench one (ruling L7: reflash, no migration):
//! erase `lpfs` over USB and the next boot formats the store.

use esp_storage::FlashStorage;
use fw_esp32_common::aligned_nor_flash::AlignedNorFlash;
use fw_esp32_common::tree_fs::littlefs_superblock_at;
use lp_bootctl::s3_mmu::{LPFS_WINDOW, LPFS_WINDOW_DROM, LPFS_WINDOW_LEN, MMU_PAGE};
use lp_tree_store::{ReadWindow, WindowedFlash};

use crate::flash_storage::LpfsPartition;

/// The store's sector: the flash's erase sector.
const SECTOR_SIZE: u32 = 4096;

/// The partition through the alignment shim (the writes, and the reads when
/// no window could be mapped).
pub type TreePart = AlignedNorFlash<FlashStorage<'static>>;

/// `lpfs` as the tree store sees it.
pub type TreeFlash = WindowedFlash<TreePart, Option<LpfsWindow>>;

/// The store's flash over the located `lpfs` partition. Records are checked
/// by the mask ROM's table-driven `crc32_le` (the store's CRC-32/ISO-HDLC),
/// as on the C6.
pub fn tree_flash(flash: FlashStorage<'static>, partition: LpfsPartition) -> TreeFlash {
    let part = AlignedNorFlash::new(flash, partition.offset(), partition.len(), SECTOR_SIZE)
        .with_crc32(esp_hal::rom::crc::crc32_le);
    let window = LpfsWindow::map(partition.offset(), partition.len());
    if window.is_none() {
        esp_println::println!("[FS] lpfs is read by ROM calls: it does not fit its cache window");
    }
    WindowedFlash::new(part, window)
}

/// Are this board's files still in littlefs? A littlefs superblock (its name
/// at byte 8 of block 0 or 1) at the start of `lpfs` — a littlefs image ran
/// on this table. Read-only, at most two 8-byte reads; asked only when the
/// store found no store on `lpfs`. (The C6 also probes the offset its old
/// table had; the S3's table never moved.)
pub fn littlefs_found(flash: &mut TreeFlash) -> bool {
    let part = flash.flash_mut();
    let lpfs = part.offset();
    littlefs_superblock_at(part.part_mut(), lpfs)
}

/// The S3 flash MMU's table (one for both buses; `split.rs` has the facts):
/// 512 words at `0x600C_5000`, bits 13:0 the flash page, bit 14 invalid.
const MMU_TABLE: usize = 0x600C_5000;
/// The cache's line.
const LINE: usize = 32;

unsafe extern "C" {
    /// The S3 mask ROM's range invalidate (`esp32s3.rom.ld`): every cache
    /// line holding a byte of `[vaddr, vaddr + size)` is dropped. 0 = done.
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
    /// Map `len` bytes of flash from `offset` at the `lpfs` window, one MMU
    /// entry per 64 KiB page; `None` when they do not fit there.
    fn map(offset: u32, len: u32) -> Option<Self> {
        let first = offset / MMU_PAGE;
        let pages = (offset + len).div_ceil(MMU_PAGE) - first;
        if pages * MMU_PAGE > LPFS_WINDOW_LEN {
            return None;
        }
        for k in 0..pages {
            let entry = LPFS_WINDOW.start + k;
            // SAFETY: an entry of the `lpfs` window, which nothing running
            // is mapped through (module docs).
            unsafe {
                core::ptr::write_volatile((MMU_TABLE + 4 * entry as usize) as *mut u32, first + k);
            }
        }
        let base = LPFS_WINDOW_DROM as usize + (offset - first * MMU_PAGE) as usize;
        // Whatever the cache held at these addresses (the loader read the
        // core through its scratch entries) is another page's bytes now.
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
/// when the ROM refused the range.
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
