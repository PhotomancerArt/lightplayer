//! The engine's window: where it is linked, and the MMU entries behind it.

use lp_bootctl::Extent;

/// Where the engine is linked (the split tool's `--engine-base`).
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

/// Map `extent` behind [`ENGINE_VADDR`], one MMU entry per page. Pages past
/// what was written read as erased flash, which the header check rejects.
pub fn map_engine(extent: Extent) {
    let page = page_size();
    let shift = page.trailing_zeros();
    let first_entry = ((ENGINE_VADDR - 0x4200_0000) >> shift) as u32;
    for k in 0..extent.len().div_ceil(page) {
        // SAFETY: entries of the engine window only, which nothing has
        // touched; the code doing it runs from the core's own pages.
        unsafe {
            core::ptr::write_volatile(MMU_ITEM_INDEX as *mut u32, first_entry + k);
            core::ptr::write_volatile(
                MMU_ITEM_CONTENT as *mut u32,
                ((extent.start >> shift) + k) | MMU_VALID,
            );
        }
    }
}
