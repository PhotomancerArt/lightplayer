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
