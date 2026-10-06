//! The engine's window: where it is linked, and the MMU entries behind it.

/// Where the engine is linked (`tools/lp-fw-split`'s `ENGINE_BASE`).
pub const ENGINE_VADDR: usize = 0x4240_0000;

const SPI0: usize = 0x6000_2000;
const MMU_ITEM_CONTENT: usize = SPI0 + 0x37c;
const MMU_ITEM_INDEX: usize = SPI0 + 0x380;
const MMU_POWER_CTRL: usize = SPI0 + 0x384;
const MMU_VALID: u32 = 1 << 9;

/// The MMU page size the IDF bootloader chose (`mmu_power_ctrl[4:3]`: 64 KiB
/// >> mode; 32 KiB on this 4 MB part). Read, never assumed.
pub fn page_size() -> u32 {
    // SAFETY: a read of an SPI0 register.
    let mode = (unsafe { core::ptr::read_volatile(MMU_POWER_CTRL as *const u32) } >> 3) & 3;
    0x1_0000 >> mode
}

/// Map `len` bytes of flash from `start` (page-aligned) behind
/// [`ENGINE_VADDR`], one MMU entry per page.
pub fn map_engine(start: u32, len: u32) {
    let page = page_size();
    let shift = page.trailing_zeros();
    let first_entry = ((ENGINE_VADDR - 0x4200_0000) >> shift) as u32;
    for k in 0..len.div_ceil(page) {
        // SAFETY: entries of the engine window only, which nothing has
        // touched; the code doing it runs from the core's own pages.
        unsafe {
            core::ptr::write_volatile(MMU_ITEM_INDEX as *mut u32, first_entry + k);
            core::ptr::write_volatile(
                MMU_ITEM_CONTENT as *mut u32,
                ((start >> shift) + k) | MMU_VALID,
            );
        }
    }
}

/// A one-off read window: flash mapped through the cache into MMU entries
/// nothing else uses, unmapped again when dropped.
///
/// The core's own hash reads its 1.2 MB through here rather than through
/// the ROM's SPI1 reads ([`super::split_flash`]), whose 64-byte calls cost
/// 392 of the 515 ms the hash took on silicon. The entries are the ones just
/// below [`ENGINE_VADDR`] — the top of the core's own window, which the core
/// (linked from `0x4200_0000` up) does not reach — and each one is checked
/// invalid before it is used, so a core grown into them makes this refuse
/// (the caller falls back to the ROM reads) rather than remap its own code.
pub struct ScratchWindow {
    first_entry: u32,
    pages: u32,
    /// `start`'s offset into its first page.
    skip: usize,
    len: usize,
}

impl ScratchWindow {
    /// Map `len` bytes of flash from `start` (any alignment), or `None` when
    /// they would need an entry already in use.
    pub fn map(start: u32, len: u32) -> Option<Self> {
        let page = page_size();
        let shift = page.trailing_zeros();
        let base = start & !(page - 1);
        let skip = start - base;
        let pages = (skip + len).div_ceil(page);
        let top = ((ENGINE_VADDR - 0x4200_0000) >> shift) as u32;
        let first_entry = top.checked_sub(pages)?;
        for k in 0..pages {
            if read_entry(first_entry + k) & MMU_VALID != 0 {
                return None;
            }
        }
        for k in 0..pages {
            write_entry(first_entry + k, ((base >> shift) + k) | MMU_VALID);
        }
        Some(Self {
            first_entry,
            pages,
            skip: skip as usize,
            len: len as usize,
        })
    }

    /// The mapped bytes, read through the cache.
    pub fn bytes(&self) -> &[u8] {
        let vaddr = 0x4200_0000 + ((self.first_entry as usize) << page_size().trailing_zeros());
        // SAFETY: `map` pointed these entries at `len` bytes of flash from
        // `skip` on; they stay mapped until `self` drops.
        unsafe { core::slice::from_raw_parts((vaddr + self.skip) as *const u8, self.len) }
    }
}

impl Drop for ScratchWindow {
    fn drop(&mut self) {
        // Invalid again, as `Cache_MMU_Init` leaves an unused entry. Nothing
        // maps these vaddrs again this boot, so no cache line of them is
        // ever read stale.
        for k in 0..self.pages {
            write_entry(self.first_entry + k, 0);
        }
    }
}

fn read_entry(index: u32) -> u32 {
    // SAFETY: selects and reads one MMU entry.
    unsafe {
        core::ptr::write_volatile(MMU_ITEM_INDEX as *mut u32, index);
        core::ptr::read_volatile(MMU_ITEM_CONTENT as *const u32)
    }
}

fn write_entry(index: u32, content: u32) {
    // SAFETY: one of `ScratchWindow`'s entries, checked unused before it
    // was taken.
    unsafe {
        core::ptr::write_volatile(MMU_ITEM_INDEX as *mut u32, index);
        core::ptr::write_volatile(MMU_ITEM_CONTENT as *mut u32, content);
    }
}

/// Where the running core really is: the flash page behind `0x4200_0000`
/// (MMU entry 0), which the loader mapped to the core's first page. The boot
/// records are only trusted when they agree with this.
pub fn running_core_offset() -> u32 {
    let shift = page_size().trailing_zeros();
    // SAFETY: selects and reads MMU entry 0.
    let entry = unsafe {
        core::ptr::write_volatile(MMU_ITEM_INDEX as *mut u32, 0);
        core::ptr::read_volatile(MMU_ITEM_CONTENT as *const u32)
    };
    (entry & (MMU_VALID - 1)) << shift
}
