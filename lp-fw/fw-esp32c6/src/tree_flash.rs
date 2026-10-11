//! The tree store's flash on the C6 (`fs-tree`): `lpfs` from the flashed
//! partition table, through the word-alignment shim
//! (`fw_esp32_common::aligned_nor_flash`, plan D10) over esp-storage.
//!
//! esp-storage on the C6 is built with `panic-unaligned-buffer` and without
//! `bytewise-read`: an offset or length that is not a multiple of 4 is
//! `NotAligned`, and a buffer at an address that is not one **panics**. The
//! store writes records at any byte offset, so every call goes through the
//! shim's aligned bounce buffer, and every program reaches the ROM
//! word-aligned and padded with `0xFF` (M4's owed "unaligned program"
//! sitting: the store's unaligned records never reach the ROM as such).
//! Programs go through `NorFlash::write`, never `Storage::write`
//! (read-erase-write of a whole sector).

use esp_storage::FlashStorage;
use fw_esp32_common::aligned_nor_flash::AlignedNorFlash;

use crate::flash_layout::PartitionExtent;

/// The store's sector: the C6 flash's erase sector.
const SECTOR_SIZE: u32 = 4096;

/// `lpfs` as the tree store sees it.
pub type TreeFlash = AlignedNorFlash<FlashStorage<'static>>;

/// The store's flash over the located `lpfs` partition.
pub fn tree_flash(flash: FlashStorage<'static>, partition: PartitionExtent) -> TreeFlash {
    AlignedNorFlash::new(flash, partition.offset, partition.len, SECTOR_SIZE)
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
        let lpfs_offset = flash.offset();
        crate::flash_storage::legacy_lpfs_present_at(flash.part_mut(), lpfs_offset)
    }
}
