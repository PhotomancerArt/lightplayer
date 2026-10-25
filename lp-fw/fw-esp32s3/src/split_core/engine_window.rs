//! The S3's flash MMU for the split image (layout 2, `lp_bootctl::s3_mmu`):
//! the engine's entries, the core's scratch window, and where the running
//! core is.
//!
//! One table of 512 words at `0x600C_5000` serves both buses: entry `n` is
//! read at `0x3C00_0000 + n × 64 KiB` (data) and fetched at
//! `0x4200_0000 + n × 64 KiB` (instructions); bits 13:0 are the flash page,
//! bit 14 means invalid. Read off the mask ROM's `Cache_MMU_Init` /
//! `Cache_*bus_MMU_Set` (lp-emu-esp32s3 `cache.rs`). After a remap the
//! caches are invalidated with the ROM's own `Cache_Invalidate_{D,I}Cache_All`
//! (the engine), or by range (the scratch window), so no line still holds
//! another page's bytes.

use lp_bootctl::s3_mmu::{CORE_SCRATCH, ENGINE_MMU_ENTRY, MMU_PAGE, drom};

const MMU_TABLE: usize = 0x600C_5000;
const MMU_INVALID: u32 = 0x4000;
const MMU_PAGE_MASK: u32 = 0x3FFF;
/// The cache's line.
const LINE: usize = 32;

unsafe extern "C" {
    #[cfg(feature = "psram")]
    fn Cache_WriteBack_All() -> i32;
    fn Cache_Invalidate_DCache_All();
    fn Cache_Invalidate_ICache_All();
    /// The S3 mask ROM's range invalidate (`esp32s3.rom.ld`). 0 = done.
    fn Cache_Invalidate_Addr(vaddr: u32, size: u32) -> i32;
}

fn mmu_read(entry: u32) -> u32 {
    // SAFETY: a read of one MMU table word.
    unsafe { core::ptr::read_volatile((MMU_TABLE + 4 * entry as usize) as *const u32) }
}

fn mmu_write(entry: u32, value: u32) {
    // SAFETY: an entry nothing running is mapped through (the engine's and
    // the core's scratch entries, never the core's own).
    unsafe { core::ptr::write_volatile((MMU_TABLE + 4 * entry as usize) as *mut u32, value) }
}

fn invalidate_caches() {
    // SAFETY: ROM routines with no arguments. Flash pages are read-only, so
    // invalidating them loses nothing — but a PSRAM page's dirty lines are
    // the only copy of what was stored there, and the engine is mapped after
    // the core brought PSRAM up (and may already have allocated in it), so a
    // `psram` image writes the data cache back first.
    unsafe {
        #[cfg(feature = "psram")]
        Cache_WriteBack_All();
        Cache_Invalidate_DCache_All();
        Cache_Invalidate_ICache_All();
    }
}

/// Map `len` bytes of flash from `start` (page-aligned) at the engine's
/// entries, both views.
pub fn map_engine(start: u32, len: u32) {
    for k in 0..len.div_ceil(MMU_PAGE) {
        mmu_write(ENGINE_MMU_ENTRY + k, start / MMU_PAGE + k);
    }
    invalidate_caches();
}

/// Where the running core is: the flash page behind MMU entry 0, which the
/// loader mapped to the core's first page. 0 when entry 0 is invalid (never
/// a core's place: the region starts at `0x20000`), so the boot records are
/// then never trusted.
pub fn running_core_offset() -> u32 {
    let e = mmu_read(0);
    if e & MMU_INVALID == 0 {
        (e & MMU_PAGE_MASK) * MMU_PAGE
    } else {
        0
    }
}

/// A one-off read window at the core's scratch entries (`0x1E0..0x200`,
/// data view `0x3DE0_0000`, 2 MiB): flash read through the cache, unmapped
/// again when dropped. The core's own hash reads through it. Each entry is
/// checked invalid before it is used, so a window still mapped refuses (the
/// caller falls back to the ROM's reads) rather than remap it.
pub struct ScratchWindow {
    pages: u32,
    /// `start`'s offset into its first page.
    skip: usize,
    len: usize,
}

impl ScratchWindow {
    /// Map `len` bytes of flash from `start` (any alignment), or `None` when
    /// they do not fit the scratch entries or one of them is in use.
    pub fn map(start: u32, len: u32) -> Option<Self> {
        let base = start & !(MMU_PAGE - 1);
        let skip = start - base;
        let pages = (skip + len).div_ceil(MMU_PAGE);
        if pages > CORE_SCRATCH.end - CORE_SCRATCH.start {
            return None;
        }
        for k in 0..pages {
            if mmu_read(CORE_SCRATCH.start + k) & MMU_INVALID == 0 {
                return None;
            }
        }
        for k in 0..pages {
            mmu_write(CORE_SCRATCH.start + k, base / MMU_PAGE + k);
        }
        let window = Self {
            pages,
            skip: skip as usize,
            len: len as usize,
        };
        // Whatever the cache held at these addresses is another page's.
        if !invalidate(window.vaddr(), window.len) {
            return None; // dropped: the entries go back to invalid
        }
        Some(window)
    }

    /// The mapped bytes, read through the cache.
    pub fn bytes(&self) -> &[u8] {
        // SAFETY: `map` pointed these entries at `len` bytes of flash from
        // `skip` on; they stay mapped until `self` drops.
        unsafe { core::slice::from_raw_parts(self.vaddr() as *const u8, self.len) }
    }

    fn vaddr(&self) -> usize {
        drom(CORE_SCRATCH.start) as usize + self.skip
    }
}

impl fw_esp32_common::split_core::ScratchWindow for ScratchWindow {
    fn map(start: u32, len: u32) -> Option<Self> {
        ScratchWindow::map(start, len)
    }

    fn bytes(&self) -> &[u8] {
        ScratchWindow::bytes(self)
    }
}

impl Drop for ScratchWindow {
    fn drop(&mut self) {
        for k in 0..self.pages {
            mmu_write(CORE_SCRATCH.start + k, MMU_INVALID);
        }
        // The next map of these entries starts from a clean cache.
        invalidate(self.vaddr(), self.len);
    }
}

/// Drop every cache line holding a byte of `[vaddr, vaddr + len)`; `false`
/// when the ROM refused the range.
fn invalidate(vaddr: usize, len: usize) -> bool {
    let start = vaddr & !(LINE - 1);
    let end = (vaddr + len).next_multiple_of(LINE);
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
    // SAFETY: a ROM routine over addresses of the scratch window.
    let refused = unsafe { Cache_Invalidate_Addr(start as u32, (end - start) as u32) } != 0;
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
    !refused
}
