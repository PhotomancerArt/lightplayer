//! Flash storage adapter for littlefs-rust.
//!
//! Implements `littlefs_rust::Storage` over `esp_storage::FlashStorage`,
//! translating block/offset addressing to the `lpfs` partition.
//!
//! Where `lpfs` is comes from the flashed partition table
//! ([`crate::flash_layout::FlashLayout`]), never a transcribed constant.

use core::sync::atomic::{AtomicU32, Ordering};

use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use littlefs_rust::{Config, Error as LfsError, Storage};

use crate::flash_layout::PartitionExtent;

/// Block size: 4KB (matches ESP32 flash sector).
const BLOCK_SIZE: u32 = 4096;
/// LittleFS read/program cache. Keep this below the erase block size so opening a
/// file does not need a transient 4KB heap allocation.
const CACHE_SIZE: u32 = 512;
/// Lookahead bitmap size. Must be a multiple of 8.
const LOOKAHEAD_SIZE: u32 = 64;

/// Flash storage adapter implementing littlefs Storage over esp_storage.
///
/// Translates littlefs block/offset addressing to absolute flash addresses
/// within the lpfs partition.
pub struct LpFlashStorage {
    flash: esp_storage::FlashStorage<'static>,
    partition: PartitionExtent,
}

impl LpFlashStorage {
    /// Create storage adapter for the located lpfs partition.
    ///
    /// Also publishes the partition's block count for [`lpfs_config`] — see
    /// that function for why the geometry has to travel through a static.
    pub fn new(flash: esp_storage::FlashStorage<'static>, partition: PartitionExtent) -> Self {
        LPFS_BLOCK_COUNT.store(partition.len / BLOCK_SIZE, Ordering::Relaxed);
        Self { flash, partition }
    }

    fn block_offset(&self, block: u32, offset: u32) -> u32 {
        self.partition.offset + block * BLOCK_SIZE + offset
    }

    /// Is a pre-repartition LightPlayer filesystem still at the legacy
    /// offset? Probed read-only (`fw_esp32_common::lp_fs::lpfs_mounts_read_only`
    /// refuses every write). Asked only after the located `lpfs` failed to
    /// mount, and never when the located partition IS the legacy one — then
    /// the failed mount was already the answer.
    pub fn legacy_lpfs_present(&mut self) -> bool {
        use crate::legacy_layout::{LEGACY_LPFS_V1_BLOCKS, LEGACY_LPFS_V1_OFFSET};
        if self.partition.offset == LEGACY_LPFS_V1_OFFSET {
            return false;
        }
        let region = RegionReader {
            flash: &mut self.flash,
            offset: LEGACY_LPFS_V1_OFFSET,
        };
        fw_esp32_common::lp_fs::lpfs_mounts_read_only(region, config_for(LEGACY_LPFS_V1_BLOCKS))
    }
}

/// A read-only littlefs view of a flash region at `offset` (the legacy
/// probe). Writes and erases are refused here as well as by the common
/// crate's wrapper.
struct RegionReader<'a> {
    flash: &'a mut esp_storage::FlashStorage<'static>,
    offset: u32,
}

impl Storage for RegionReader<'_> {
    fn read(&mut self, block: u32, offset: u32, buf: &mut [u8]) -> Result<(), LfsError> {
        let addr = self.offset + block * BLOCK_SIZE + offset;
        self.flash.read(addr, buf).map_err(|_| LfsError::Io)
    }

    fn write(&mut self, _block: u32, _offset: u32, _data: &[u8]) -> Result<(), LfsError> {
        Err(LfsError::Io)
    }

    fn erase(&mut self, _block: u32) -> Result<(), LfsError> {
        Err(LfsError::Io)
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
        let from = self.partition.offset + block * BLOCK_SIZE;
        let to = from + BLOCK_SIZE;
        self.flash.erase(from, to).map_err(|_| LfsError::Io)
    }
}

/// Block count published by [`LpFlashStorage::new`], read back by
/// [`lpfs_config`].
static LPFS_BLOCK_COUNT: AtomicU32 = AtomicU32::new(0);

/// littlefs configuration for the lpfs partition.
///
/// `LpFsFlash::init` takes the config factory as a bare `fn() -> Config` — a
/// function pointer, with nowhere to put a captured partition — while the C6
/// reads its geometry from the flashed partition table at runtime rather than
/// from a transcribed constant (see the module docs for why). The two are
/// bridged by a static that [`LpFlashStorage::new`] fills in, which is sound
/// because the storage adapter is always constructed before being handed to
/// `init`, on the one thread that exists at boot.
///
/// A zero block count means this ran before any adapter was built; littlefs
/// rejects it rather than mounting a zero-length filesystem, so the failure is
/// loud.
pub fn lpfs_config() -> Config {
    config_for(LPFS_BLOCK_COUNT.load(Ordering::Relaxed))
}

/// The firmware's littlefs geometry for a region of `block_count` blocks.
fn config_for(block_count: u32) -> Config {
    let mut config = Config::new(BLOCK_SIZE, block_count);
    config.cache_size = CACHE_SIZE;
    config.lookahead_size = LOOKAHEAD_SIZE;
    config
}
