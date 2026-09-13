//! The SPI NOR flash chip on the other side of `SPI1`, and the S3's own
//! partition facts.
//!
//! The chip model itself lives in
//! [`lp_emu_esp_common::engine::spi_flash`] (M2 P6) — a NOR flash is a NOR
//! flash on every part. Program is an `&=` so a double write without an
//! erase shows up, erase is the only way back to `0xff`, the JEDEC capacity
//! byte is derived from the same length the ROM's `chip_size` word is, and
//! `--flash` / `--flash-copy` / blank are the three persistence policies
//! (four with the host-owned `Bytes`). The engine's own tests say all of
//! that; nothing here re-states it.
//!
//! What stays here is the numbers that are **this board's**:
//!
//! ```text
//! 0x000000  bootloader        (espflash's bundled ESP-IDF second stage)
//! 0x008000  partition table   (magic 0xaa50)
//! 0x009000  nvs      0x5000
//! 0x00e000  bootctl  0x1000
//! 0x00f000  phy_init 0x1000
//! 0x010000  factory  0x600000  (6 MB, 0x010000..0x610000)
//! 0x610000  lpfs     0x180000  (1.5 MB, 0x610000..0x790000)
//! 0x790000  end — 448 KB of slack under the 8 MB chip
//! ```
//!
//! from `lp-fw/fw-esp32s3/partitions.csv` plus the two offsets the ESP-IDF
//! layout fixes for this chip.
//!
//! # ⚠️ 8 MB, and the mirror is the contract
//!
//! The chip is **8 MB**, not the 4 MB both other machines model:
//! `lp-fw/fw-esp32s3/partitions.csv` deliberately does not fit a 4 MB board
//! (`docs/adr/2026-07-30-esp32s3-partition-floor.md`). The flash-size string
//! `8mb` has three mirrors that cannot read each other —
//! `lp-fw/fw-esp32s3/.cargo/config.toml`'s runner, the `justfile`'s
//! `s3_flash_size`, and the canonical `lp-fw/builds/esp32s3-8mb.json`
//! (`flashSizeMb`) — and [`DEFAULT_FLASH_LEN`] is the fourth. None of the
//! four can read another; the mirror **is** the contract, and
//! `the_partition_facts_fit_the_default_chip` is the test that holds this
//! one to the partition table.
//!
//! # ⚠️ The bootloader is at `0x0`, not the classic's `0x1000`
//!
//! Read off the merged image, not chosen: `espflash save-image --chip esp32s3
//! --merge` writes the ESP-IDF second-stage bootloader at offset **`0x0`**
//! (the image's first byte is the `0xe9` magic; the header carries chip id
//! **9**, ESP32-S3, entry `0x403c_9908`). The classic reserves its first 4 KiB
//! and puts the bootloader at `0x1000`; a parser that used the classic's
//! constant here would read the middle of the bootloader's first segment as
//! a header and conclude "nothing was flashed". `tests/rom_up_boot.rs`
//! checks the merged image it is handed against this offset.

pub use lp_emu_esp_common::engine::spi_flash::{
    BLOCK_LEN, FlashBacking, FlashCensus, FlashHandle, FlashImage, PAGE_LEN, SECTOR_LEN,
};

/// The flash the S3 board has, and the size `justfile`'s `s3_flash_size`
/// flashes with: **8 MiB**. `--flash-len` overrides. See the module docs for
/// the three mirrors this is the fourth of.
pub const DEFAULT_FLASH_LEN: u32 = 8 * 1024 * 1024;

/// Where a flasher puts the second-stage bootloader on the **S3**: `0x0`.
/// Read off the merged image (module docs), not the classic's `0x1000`.
pub const BOOTLOADER_OFFSET: u32 = 0x0000_0000;

/// Where a flasher puts the partition table (ESP-IDF's default for this
/// chip, and what `just flash-fw-esp32s3` writes).
pub const PARTITION_TABLE_OFFSET: u32 = 0x0000_8000;

/// `nvs` (`lp-fw/fw-esp32s3/partitions.csv`).
pub const NVS_OFFSET: u32 = 0x0000_9000;
pub const NVS_LEN: u32 = 0x0000_5000;

/// `bootctl` — the boot-control sector carved out of `nvs`
/// (`docs/adr/2026-07-30-boot-control-sector.md`).
pub const BOOTCTL_OFFSET: u32 = 0x0000_E000;
pub const BOOTCTL_LEN: u32 = 0x0000_1000;

/// `phy_init`.
pub const PHY_INIT_OFFSET: u32 = 0x0000_F000;
pub const PHY_INIT_LEN: u32 = 0x0000_1000;

/// The `factory` partition's offset, where a flashed app image starts.
pub const FACTORY_OFFSET: u32 = 0x0001_0000;

/// The `factory` partition's length — **6 MiB** on this chip.
pub const FACTORY_LEN: u32 = 0x0060_0000;

/// The `lpfs` partition (`partitions.csv`, and
/// `fw-esp32s3`'s own flash-storage constant).
pub const LPFS_OFFSET: u32 = 0x0061_0000;
pub const LPFS_LEN: u32 = 0x0018_0000;

#[cfg(test)]
mod tests {
    use super::*;

    /// The two consumers of the chip's size have to agree: `esp_storage`
    /// decodes the JEDEC capacity byte out of `flash_rdid`, and the mask ROM
    /// refuses a read past its `chip_size` word
    /// ([`crate::loader::seed_rom_flash_chip`]).
    #[test]
    fn the_default_chips_capacity_byte_describes_the_default_length() {
        let f = FlashImage::blank(DEFAULT_FLASH_LEN);
        let [manufacturer, memory_type, capacity, _] = f.jedec_id().to_le_bytes();
        assert_eq!(1u32 << capacity, DEFAULT_FLASH_LEN);
        assert_eq!(capacity, 0x17, "8 MiB is 2^23");
        // The ROM's own default part, `.data_spi_flash`'s `device_id`
        // (`0x001540ef` at `0x3fcef6a4`), with this chip's capacity byte in
        // place of its own.
        assert_eq!((manufacturer, memory_type), (0xef, 0x40));
    }

    /// The partition table this firmware flashes fits the chip with the
    /// 448 KB of slack the CSV's own comment names, and the fixed offsets sit
    /// below `factory` in the order a flasher writes them.
    #[test]
    fn the_partition_facts_fit_the_default_chip() {
        assert!(BOOTLOADER_OFFSET < PARTITION_TABLE_OFFSET);
        assert!(PARTITION_TABLE_OFFSET < NVS_OFFSET);
        assert_eq!(NVS_OFFSET + NVS_LEN, BOOTCTL_OFFSET);
        assert_eq!(BOOTCTL_OFFSET + BOOTCTL_LEN, PHY_INIT_OFFSET);
        assert_eq!(PHY_INIT_OFFSET + PHY_INIT_LEN, FACTORY_OFFSET);
        assert_eq!(FACTORY_OFFSET + FACTORY_LEN, LPFS_OFFSET);
        assert_eq!(LPFS_OFFSET + LPFS_LEN, 0x0079_0000);
        assert_eq!(
            DEFAULT_FLASH_LEN - (LPFS_OFFSET + LPFS_LEN),
            448 * 1024,
            "the CSV's own `448 KB slack`"
        );
        assert!(FACTORY_OFFSET.is_multiple_of(BLOCK_LEN));
        assert!(LPFS_OFFSET.is_multiple_of(BLOCK_LEN));
        // The table does NOT fit a 4 MB board, on purpose (the partition
        // floor ADR): this is the assertion that keeps the 8 MB mirror honest.
        assert!(LPFS_OFFSET + LPFS_LEN > 4 * 1024 * 1024);
    }
}
