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

/// The flash size the C6 boards ship with, and the size
/// `lp-fw/fw-esp32c6/partitions.csv` fills exactly (`lpfs` ends at
/// `0x310000 + 0xF0000 = 0x400000`).
pub const DEFAULT_FLASH_LEN: u32 = 4 * 1024 * 1024;

/// The `factory` partition's offset (`partitions.csv`), where a flashed app
/// image starts.
pub const FACTORY_OFFSET: u32 = 0x0001_0000;

/// The `lpfs` partition (`partitions.csv`, and hardcoded in
/// `fw-esp32c6/src/flash_storage.rs` as `LPFS_PARTITION_OFFSET`).
pub const LPFS_OFFSET: u32 = 0x0031_0000;
pub const LPFS_LEN: u32 = 0x000F_0000;

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
    }
}
