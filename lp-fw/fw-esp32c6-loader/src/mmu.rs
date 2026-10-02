//! The flash MMU, through SPI0's two indirect registers.

const SPI0: usize = 0x6000_2000;
const MMU_ITEM_CONTENT: usize = SPI0 + 0x37c;
const MMU_ITEM_INDEX: usize = SPI0 + 0x380;
const MMU_POWER_CTRL: usize = SPI0 + 0x384;
const MMU_VALID: u32 = 1 << 9;

/// The cached flash window every entry maps into.
pub const WINDOW: core::ops::Range<u32> = 0x4200_0000..0x4300_0000;

/// log2 of the page size the IDF bootloader chose (`mmu_power_ctrl[4:3]`:
/// 64 KiB >> mode; 32 KiB on the 4 MB C6 this ships on).
pub fn page_shift() -> u32 {
    // SAFETY: a read of an SPI0 register.
    16 - ((unsafe { core::ptr::read_volatile(MMU_POWER_CTRL as *const u32) } >> 3) & 3)
}

/// Map the page at virtual `vaddr` to the flash page at `paddr` (both
/// page-aligned).
pub fn map(vaddr: u32, paddr: u32, page_shift: u32) {
    let entry = (vaddr - WINDOW.start) >> page_shift;
    // SAFETY: SPI0's MMU registers; nothing runs from the window.
    unsafe {
        core::ptr::write_volatile(MMU_ITEM_INDEX as *mut u32, entry);
        core::ptr::write_volatile(
            MMU_ITEM_CONTENT as *mut u32,
            (paddr >> page_shift) | MMU_VALID,
        );
    }
}
