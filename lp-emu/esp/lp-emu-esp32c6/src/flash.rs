//! The SPI NOR flash chip on the other side of SPI1.
//!
//! The chip model itself lives in
//! [`lp_emu_esp_common::engine::spi_flash`] — a NOR flash is a NOR flash on
//! every part, and the classic and the S3 boot off one too (M2 P6). Program
//! is still an `&=`, erase is still the only way back to `0xff`, the JEDEC
//! capacity byte is still derived from the same length the ROM's `chip_size`
//! word is, and `--flash` / `--flash-copy` / blank are still the three
//! persistence policies; the engine's own tests say so.
//!
//! What stays here is the numbers that are **C6 board and partition facts**,
//! which is why this module still exists rather than the crate importing the
//! engine directly: every `crate::flash::…` path in `machine.rs`,
//! `loader.rs`, `cache.rs`, `periph/` and the integration tests resolves
//! exactly as it did before the move.

pub use lp_emu_esp_common::engine::spi_flash::{
    BLOCK_LEN, FlashBacking, FlashCensus, FlashHandle, FlashImage, PAGE_LEN, SECTOR_LEN,
};

/// The first byte of an ESP image header. A chip whose reset vector starts
/// with it has something to boot; a part full of `0xff` does not.
///
/// A **board** fact rather than a part fact, which is why it is here and not
/// in the engine: the NOR chip does not care what is written to it, and it
/// is the ROM that decides these four bytes mean an image.
pub const ESP_IMAGE_MAGIC: u8 = 0xe9;

/// The flash size the C6 boards ship with, and the size
/// `lp-fw/fw-esp32c6/partitions.csv` fills exactly (`lpfs` ends at
/// `LPFS_OFFSET + LPFS_LEN = 0x400000`).
pub const DEFAULT_FLASH_LEN: u32 = 4 * 1024 * 1024;

/// Where a flasher writes the partition table. Re-exported from
/// [`crate::image`] so the table's facts read as one list here.
pub use crate::image::PARTITION_TABLE_OFFSET;

/// The `nvs` partition (`partitions.csv`). Nothing reads it; listed because
/// the table a direct load stages must be the whole table, row for row.
pub const NVS_OFFSET: u32 = 0x0000_9000;
pub const NVS_LEN: u32 = 0x0000_5000;

/// The `bootctl` sector (`partitions.csv`,
/// `docs/adr/2026-07-30-boot-control-sector.md`).
pub const BOOTCTL_OFFSET: u32 = 0x0000_e000;
pub const BOOTCTL_LEN: u32 = 0x0000_1000;

/// The `phy_init` partition (`partitions.csv`).
pub const PHY_INIT_OFFSET: u32 = 0x0000_f000;
pub const PHY_INIT_LEN: u32 = 0x0000_1000;

/// The `factory` partition (`partitions.csv`), where a flashed app image
/// starts, and how long it may be.
pub const FACTORY_OFFSET: u32 = 0x0001_0000;
pub const FACTORY_LEN: u32 = 0x0030_0000;

/// The `lpfs` partition (`partitions.csv`). The firmware reads it from the
/// flashed table at boot (`fw-esp32c6/src/flash_storage.rs`), so a direct
/// load stages [`c6_partition_table_bytes`] for it to find.
pub const LPFS_OFFSET: u32 = 0x0031_0000;
pub const LPFS_LEN: u32 = 0x000F_0000;

/// The length of a compiled partition table: rows, the MD5 row, then `0xff`
/// to here (ESP-IDF's `PARTITION_TABLE_MAX_LEN`, and what espflash writes).
pub const PARTITION_TABLE_LEN: usize = 0xC00;

/// `partitions.csv` compiled to the binary table espflash writes at
/// [`PARTITION_TABLE_OFFSET`], byte for byte.
///
/// A **board model's** facts, not a firmware transcription: the firmware
/// reads its layout from whatever table is flashed, and this is the table a
/// flasher would have flashed. `lp-cli`'s parity test
/// (`lp-cli/tests/c6_partition_table_parity.rs`) compiles the real
/// `partitions.csv` with espflash's own encoder and asserts it equals these
/// bytes — the fence keeps this crate from reading the product's file, so
/// the product side owns the check.
///
/// The encoding (ESP-IDF's `gen_esp32part.py`, as espflash's `esp-idf-part`
/// emits it): 32-byte rows — `0xAA 0x50`, type, subtype, offset LE, size LE,
/// a NUL-padded 16-byte label, flags LE — then an MD5 row (`0xEB 0xEB`,
/// fourteen `0xff`, the MD5 of every preceding row), then `0xff` to
/// [`PARTITION_TABLE_LEN`]. The firmware's `esp-bootloader-esp-idf` verifies
/// that MD5, so it has to be right.
pub fn c6_partition_table_bytes() -> [u8; PARTITION_TABLE_LEN] {
    // (label, type, subtype, offset, len)
    const ROWS: [(&str, u8, u8, u32, u32); 5] = [
        ("nvs", 0x01, 0x02, NVS_OFFSET, NVS_LEN),
        ("bootctl", 0x01, 0x06, BOOTCTL_OFFSET, BOOTCTL_LEN),
        ("phy_init", 0x01, 0x01, PHY_INIT_OFFSET, PHY_INIT_LEN),
        ("factory", 0x00, 0x00, FACTORY_OFFSET, FACTORY_LEN),
        ("lpfs", 0x01, 0x82, LPFS_OFFSET, LPFS_LEN),
    ];
    let mut out = [0xffu8; PARTITION_TABLE_LEN];
    let mut at = 0usize;
    for (label, kind, subtype, offset, len) in ROWS {
        let row = &mut out[at..at + 32];
        row[0] = 0xAA;
        row[1] = 0x50;
        row[2] = kind;
        row[3] = subtype;
        row[4..8].copy_from_slice(&offset.to_le_bytes());
        row[8..12].copy_from_slice(&len.to_le_bytes());
        row[12..28].fill(0);
        row[12..12 + label.len()].copy_from_slice(label.as_bytes());
        row[28..32].copy_from_slice(&0u32.to_le_bytes());
        at += 32;
    }
    let digest = {
        use md5::{Digest, Md5};
        Md5::digest(&out[..at])
    };
    // The MD5 row: the magic, fourteen 0xff (already there), the digest.
    out[at] = 0xEB;
    out[at + 1] = 0xEB;
    out[at + 16..at + 32].copy_from_slice(&digest);
    out
}

/// Does the chip already hold a partition table? A direct load stages
/// [`c6_partition_table_bytes`] only onto a chip that does not — a `--flash`
/// chip that was really flashed keeps its own (that is the layout a
/// migration test is about).
pub fn holds_partition_table(chip: &[u8]) -> bool {
    let at = PARTITION_TABLE_OFFSET as usize;
    chip.get(at..at + 2) == Some(&[0xAA, 0x50][..])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two consumers of the chip's size have to agree, and this crate is
    /// where the C6's own number is: `esp_storage` decodes the JEDEC
    /// capacity byte, and the mask ROM refuses a read past its `chip_size`
    /// word (see [`crate::loader::seed_rom_flash_chip`]).
    #[test]
    fn the_default_chips_capacity_byte_describes_the_default_length() {
        let f = FlashImage::blank(DEFAULT_FLASH_LEN);
        let [_, _, capacity, _] = f.jedec_id().to_le_bytes();
        assert_eq!(1u32 << capacity, DEFAULT_FLASH_LEN);
    }

    /// The partition table this firmware flashes: `factory` at `0x10000`,
    /// `lpfs` last and ending exactly at the end of the chip.
    #[test]
    fn the_partition_facts_fill_the_default_chip_exactly() {
        assert!(FACTORY_OFFSET < LPFS_OFFSET);
        assert_eq!(LPFS_OFFSET + LPFS_LEN, DEFAULT_FLASH_LEN);
        assert!(LPFS_OFFSET.is_multiple_of(BLOCK_LEN));
        assert_eq!(FACTORY_OFFSET + FACTORY_LEN, LPFS_OFFSET);
    }

    /// The staged table reads back as the same rows through this crate's
    /// own reader, and carries its MD5 row where a compiled table does.
    #[test]
    fn the_synthesized_table_reads_back_as_the_partition_facts() {
        let mut chip = vec![0xffu8; DEFAULT_FLASH_LEN as usize];
        assert!(!holds_partition_table(&chip));
        let table = c6_partition_table_bytes();
        let at = PARTITION_TABLE_OFFSET as usize;
        chip[at..at + table.len()].copy_from_slice(&table);
        assert!(holds_partition_table(&chip));
        let got: Vec<(String, u32, u32)> = crate::image::partitions(&chip)
            .into_iter()
            .map(|p| (p.label, p.offset, p.len))
            .collect();
        let want: Vec<(String, u32, u32)> = [
            ("nvs", NVS_OFFSET, NVS_LEN),
            ("bootctl", BOOTCTL_OFFSET, BOOTCTL_LEN),
            ("phy_init", PHY_INIT_OFFSET, PHY_INIT_LEN),
            ("factory", FACTORY_OFFSET, FACTORY_LEN),
            ("lpfs", LPFS_OFFSET, LPFS_LEN),
        ]
        .into_iter()
        .map(|(l, o, n)| (l.to_string(), o, n))
        .collect();
        assert_eq!(got, want);
        assert_eq!(&table[5 * 32..5 * 32 + 2], &[0xEB, 0xEB]);
    }
}
