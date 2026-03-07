//! Flash storage adapter for littlefs-rust.
//!
//! Implements `littlefs_rust::Storage` over `esp_storage::FlashStorage`,
//! translating block/offset addressing to the lpfs partition at 0x310000.

use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use littlefs_rust::{Config, Error as LfsError, Storage};

/// lpfs partition offset (from partitions.csv)
const LPFS_PARTITION_OFFSET: u32 = 0x310000;
/// Block size: 4KB (matches ESP32 flash sector)
const BLOCK_SIZE: u32 = 4096;
/// 960KB partition = 240 blocks
const BLOCK_COUNT: u32 = 240;

/// Flash storage adapter implementing littlefs Storage over esp_storage.
///
/// Translates littlefs block/offset addressing to absolute flash addresses
/// within the lpfs partition.
pub struct LpFlashStorage {
    flash: esp_storage::FlashStorage<'static>,
}

impl LpFlashStorage {
    /// Create storage adapter for the lpfs partition.
    pub fn new(flash: esp_storage::FlashStorage<'static>) -> Self {
        Self { flash }
    }

    fn block_offset(&self, block: u32, offset: u32) -> u32 {
        LPFS_PARTITION_OFFSET + block * BLOCK_SIZE + offset
    }
}

impl Storage for LpFlashStorage {
    fn read(&mut self, block: u32, offset: u32, buf: &mut [u8]) -> Result<(), LfsError> {
        let addr = self.block_offset(block, offset);
        self.flash.read(addr, buf).map_err(|_| LfsError::Io)
    }

    fn write(&mut self, block: u32, offset: u32, data: &[u8]) -> Result<(), LfsError> {
        let addr = self.block_offset(block, offset);
        self.flash.write(addr, data).map_err(|_| LfsError::Io)
    }

    fn erase(&mut self, block: u32) -> Result<(), LfsError> {
        let from = LPFS_PARTITION_OFFSET + block * BLOCK_SIZE;
        let to = from + BLOCK_SIZE;
        self.flash.erase(from, to).map_err(|_| LfsError::Io)
    }
}

/// littlefs configuration for the lpfs partition.
pub fn lpfs_config() -> Config {
    Config::new(BLOCK_SIZE, BLOCK_COUNT)
}
