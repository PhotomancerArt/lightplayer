//! The tree store's flash on the C6: `lpfs` from the flashed partition
//! table, through the word-alignment shim
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
//!
//! # littlefs, by its magic only
//!
//! littlefs left this firmware with the tree-store cutover (plan
//! `lp2025/2026-10-16-0848-tree-store-adoption`, AD5): nothing here mounts
//! it. A board whose files are still in littlefs is recognised by the
//! superblock's name alone ([`littlefs_found`]) and HELD — never formatted
//! (AD3) — until the USB migration moves them
//! (`docs/adr/2026-10-16-c6-tree-store-cutover-migration.md`).

use esp_storage::FlashStorage;
use fw_esp32_common::aligned_nor_flash::AlignedNorFlash;
use fw_esp32_common::tree_fs::littlefs_superblock_at;

use crate::flash_layout::PartitionExtent;

/// The store's sector: the C6 flash's erase sector.
const SECTOR_SIZE: u32 = 4096;

/// Where the 2026-10 repartition's 176-sector littlefs `lpfs` starts — the
/// table every board fielded before the cutover carries
/// (`lp-app/lpa-link/testdata/partitions-esp32c6-legacy-v2.csv`). A
/// **historical fact**, never a mirror of `partitions.csv`.
///
/// Today's `lpfs` (`0x380000`) is that filesystem's blocks 48–175, so a
/// board flashed with the tree table by a path that skipped the migration
/// finds no store at `0x380000` and must not format over them. Its
/// superblock survives such a flash only while `app.bin` ends below this
/// offset (`lp-fw-split headroom` reports the margin as "legacy overlap").
/// The pre-repartition littlefs at `0x310000` is not probed any more: every
/// image since the 2026-10 repartition writes over it.
pub const LITTLEFS_V2_OFFSET: u32 = 0x0035_0000;

/// `lpfs` as the tree store sees it.
pub type TreeFlash = AlignedNorFlash<FlashStorage<'static>>;

/// The store's flash over the located `lpfs` partition. Records are checked
/// by the mask ROM's table-driven `crc32_le` (FORMAT.md's CRC-32/ISO-HDLC,
/// the same values as `lp-crc32`): ~8x fewer cycles a byte than the bitwise
/// one, which bounds mount and every record read, for no flash.
pub fn tree_flash(flash: FlashStorage<'static>, partition: PartitionExtent) -> TreeFlash {
    AlignedNorFlash::new(flash, partition.offset, partition.len, SECTOR_SIZE)
        .with_crc32(esp_hal::rom::crc::crc32_le)
}

/// Are this board's files still in littlefs? A littlefs superblock (its
/// name at byte 8 of block 0 or 1) at the start of `lpfs` itself — a
/// littlefs core ran on this table — or at [`LITTLEFS_V2_OFFSET`], a
/// fielded board flashed past its migration. Read-only, at most four 8-byte
/// reads; asked only when the store found no store on `lpfs`. littlefs is
/// not linked: whether the files are whole is the USB migration's question
/// (it mounts them on the host), and holding costs nothing if they are not.
pub fn littlefs_found(flash: &mut TreeFlash) -> bool {
    let lpfs = flash.offset();
    let part = flash.part_mut();
    littlefs_superblock_at(part, lpfs)
        || (lpfs != LITTLEFS_V2_OFFSET && littlefs_superblock_at(part, LITTLEFS_V2_OFFSET))
}
