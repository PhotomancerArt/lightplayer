//! The S3's flash MMU: one table of 512 words, one entry per 64 KiB page,
//! serving the data bus and the instruction bus at the same index
//! (`lp_bootctl::s3_mmu`). Written directly: the mask ROM's own setters
//! (`Cache_Ibus_MMU_Set`/`Cache_Dbus_MMU_Set`) write the same words.

use lp_bootctl::s3_mmu::{LOADER_SCRATCH, MMU_PAGE};

/// The table (`Cache_MMU_Init` fills `0x600C_5000..0x600C_5800`; modelled
/// in `lp-emu-esp32s3`'s `cache.rs` from the ROM's code).
const MMU_TABLE: usize = 0x600C_5000;
/// An unmapped entry: bit 14, what `Cache_MMU_Init` writes into every slot.
pub const MMU_INVALID: u32 = 0x4000;

/// Map table entry `entry` to the flash page at `paddr` (page-aligned).
pub fn map(entry: u32, paddr: u32) {
    set(entry, paddr / MMU_PAGE);
}

/// Unmap every entry of the loader's scratch window (ruling L4: the core
/// starts with that window empty).
pub fn unmap_scratch() {
    for entry in LOADER_SCRATCH {
        set(entry, MMU_INVALID);
    }
}

fn set(entry: u32, word: u32) {
    // SAFETY: one MMU table word; the loader runs from RAM, never from a
    // page it remaps.
    unsafe { core::ptr::write_volatile((MMU_TABLE + 4 * entry as usize) as *mut u32, word) }
}
