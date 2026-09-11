//! The SPI NOR flash chip on the other side of SPI1, and the classic's own
//! partition facts.
//!
//! The chip model itself lives in
//! [`lp_emu_esp_common::engine::spi_flash`] (M2 P6) — a NOR flash is a NOR
//! flash on every part. Program is an `&=` so a double write without an
//! erase shows up, erase is the only way back to `0xff`, the JEDEC capacity
//! byte is derived from the same length the ROM's `chip_size` word is, and
//! `--flash` / `--flash-copy` / blank are the three persistence policies.
//! The engine's own tests say all of that; nothing here re-states it.
//!
//! What stays here is the numbers that are **this board's**, and they are
//! the desk board's (`../bench.md`: DOM-Z-102, "4 MB flash"):
//!
//! ```text
//! 0x001000  bootloader        (espflash's bundled ESP-IDF second stage)
//! 0x008000  partition table   (magic 0xaa50)
//! 0x009000  nvs      0x6000
//! 0x00f000  phy_init 0x1000
//! 0x010000  factory  0x300000
//! 0x310000  lpfs     0xf0000   → ends exactly at 0x400000
//! ```
//!
//! from `lp-fw/fw-esp32v3/partitions.csv` plus the two offsets the ESP-IDF
//! layout fixes for this chip. ⚠️ **The classic's bootloader is at
//! `0x1000`, not at `0x0`** — the C6's is at `0x0`, and a parser that used
//! the C6's constant would find `0xff` and conclude "nothing was flashed".

pub use lp_emu_esp_common::engine::spi_flash::{
    BLOCK_LEN, FlashBacking, FlashCensus, FlashHandle, FlashImage, PAGE_LEN, SECTOR_LEN,
};

/// The flash the desk board has, and the size `justfile`'s `v3_flash_size`
/// flashes with: **4 MiB**. `--flash-len` overrides.
pub const DEFAULT_FLASH_LEN: u32 = 4 * 1024 * 1024;

/// Where a flasher puts the second-stage bootloader on the **classic**.
///
/// `0x1000`, not the C6's `0x0`: the classic reserves the first 4 KiB, and
/// L0's own capture proves it — the ROM's `load:0x3fff0030,len:7104` line is
/// the first segment of the image *at this offset*.
pub const BOOTLOADER_OFFSET: u32 = 0x0000_1000;

/// Where a flasher puts the partition table (ESP-IDF's default for this
/// chip, and what `just flash-fw-esp32v3` writes).
pub const PARTITION_TABLE_OFFSET: u32 = 0x0000_8000;

/// The `factory` partition's offset (`lp-fw/fw-esp32v3/partitions.csv`),
/// where a flashed app image starts.
pub const FACTORY_OFFSET: u32 = 0x0001_0000;

/// The `factory` partition's length — 3 MiB.
pub const FACTORY_LEN: u32 = 0x0030_0000;

/// The `lpfs` partition (`partitions.csv`, and
/// `fw-esp32v3/src/flash_storage.rs`'s own constant).
pub const LPFS_OFFSET: u32 = 0x0031_0000;
pub const LPFS_LEN: u32 = 0x000F_0000;

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
        // The ROM's own default part, `.data_spi_flash`'s `device_id`
        // (`0x001540ef`), with this chip's capacity byte in place of its own.
        assert_eq!((manufacturer, memory_type), (0xef, 0x40));
    }

    /// The partition table this firmware flashes fills the chip exactly, and
    /// the two fixed offsets sit below `factory`.
    #[test]
    fn the_partition_facts_fill_the_default_chip_exactly() {
        assert!(BOOTLOADER_OFFSET < PARTITION_TABLE_OFFSET);
        assert!(PARTITION_TABLE_OFFSET < FACTORY_OFFSET);
        assert_eq!(FACTORY_OFFSET + FACTORY_LEN, LPFS_OFFSET);
        assert_eq!(LPFS_OFFSET + LPFS_LEN, DEFAULT_FLASH_LEN);
        assert!(FACTORY_OFFSET.is_multiple_of(BLOCK_LEN));
        assert!(LPFS_OFFSET.is_multiple_of(BLOCK_LEN));
    }
}
