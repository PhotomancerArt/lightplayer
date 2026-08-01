//! Flash storage adapter for littlefs-rust.
//!
//! Implements `littlefs_rust::Storage` over `esp_storage::FlashStorage`,
//! translating block/offset addressing to the `lpfs` partition.
//!
//! ## Divergence from the C6 (deliberate)
//!
//! `fw-esp32c6/src/flash_storage.rs` hardcodes `LPFS_PARTITION_OFFSET =
//! 0x310000` and `BLOCK_COUNT = 240`, transcribed by hand from that chip's
//! `partitions.csv`. Copying those constants here would be actively dangerous:
//! the S3's 8 MB table (M3 P1) puts `lpfs` at `0x610000`, so the C6 values
//! point into the middle of this chip's **factory** partition — a filesystem
//! mount would silently erase running code, with no diagnostic.
//!
//! So the offset and length are read from the partition table at runtime,
//! matched by the `lpfs` label. `esp-bootloader-esp-idf` is already a
//! dependency (`esp_app_desc!`), the table is the same artifact espflash
//! writes, and there is no second copy of the layout to keep in sync.

use core::sync::atomic::{AtomicU32, Ordering};

use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use esp_bootloader_esp_idf::partitions;
use littlefs_rust::{Config, Error as LfsError, Storage};

/// Partition label to mount as the LightPlayer filesystem. Must match
/// `partitions.csv`.
const LPFS_LABEL: &str = "lpfs";

/// Block size: 4KB (matches ESP32 flash sector).
const BLOCK_SIZE: u32 = 4096;
/// LittleFS read/program cache. Keep this below the erase block size so opening a
/// file does not need a transient 4KB heap allocation.
const CACHE_SIZE: u32 = 512;
/// Lookahead bitmap size. Must be a multiple of 8.
const LOOKAHEAD_SIZE: u32 = 64;

/// Where the `lpfs` partition actually is, as read from the flashed partition
/// table rather than transcribed from `partitions.csv`.
#[derive(Debug, Clone, Copy)]
pub struct LpfsPartition {
    offset: u32,
    len: u32,
}

impl LpfsPartition {
    /// Locate the `lpfs` partition by label.
    ///
    /// Returns `None` when the table has no such entry — which means the image
    /// was flashed without `--partition-table lp-fw/fw-esp32s3/partitions.csv`
    /// and espflash substituted its default table. That is a flashing mistake,
    /// not a runtime condition, so the caller should say so loudly rather than
    /// fall back to a guessed offset.
    pub fn locate(flash: &mut impl embedded_storage::Storage) -> Option<Self> {
        let mut buf = [0u8; partitions::PARTITION_TABLE_MAX_LEN];
        let table = partitions::read_partition_table(flash, &mut buf).ok()?;
        let entry = table.iter().find(|e| e.label_as_str() == LPFS_LABEL)?;
        Some(Self {
            offset: entry.offset(),
            len: entry.len(),
        })
    }

    /// Number of 4KB littlefs blocks in the partition.
    pub fn block_count(&self) -> u32 {
        self.len / BLOCK_SIZE
    }
}

/// Flash storage adapter implementing littlefs Storage over esp_storage.
///
/// Translates littlefs block/offset addressing to absolute flash addresses
/// within the lpfs partition.
pub struct LpFlashStorage {
    flash: esp_storage::FlashStorage<'static>,
    partition: LpfsPartition,
}

impl LpFlashStorage {
    /// Create storage adapter for the located lpfs partition.
    ///
    /// Also publishes the partition's block count for [`lpfs_config`] — see
    /// that function for why the geometry has to travel through a static.
    pub fn new(flash: esp_storage::FlashStorage<'static>, partition: LpfsPartition) -> Self {
        LPFS_BLOCK_COUNT.store(partition.block_count(), Ordering::Relaxed);
        Self { flash, partition }
    }

    fn block_offset(&self, block: u32, offset: u32) -> u32 {
        self.partition.offset + block * BLOCK_SIZE + offset
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
/// function pointer, with nowhere to put a captured partition — while the S3
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
    let mut config = Config::new(BLOCK_SIZE, LPFS_BLOCK_COUNT.load(Ordering::Relaxed));
    config.cache_size = CACHE_SIZE;
    config.lookahead_size = LOOKAHEAD_SIZE;
    config
}
