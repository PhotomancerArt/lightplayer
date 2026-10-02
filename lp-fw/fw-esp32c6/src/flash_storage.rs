//! Flash storage adapter for littlefs-rust.
//!
//! Implements `littlefs_rust::Storage` over `esp_storage::FlashStorage`,
//! translating block/offset addressing to the `lpfs` partition.
//!
//! ## Where `lpfs` is comes from the flashed table
//!
//! The offset and length are read from the partition table at runtime,
//! matched by the `lpfs` label — the same pattern as `fw-esp32s3` and
//! `fw-esp32v3`, so all three chips now read their table. Until 2026-10 this
//! module hardcoded `0x310000` / 240 blocks, transcribed by hand from
//! `partitions.csv` (`docs/debt/firmware-partition-constants-transcribed.md`,
//! paid). A transcribed offset that drifts from the flashed table does not
//! fail, it mounts across whatever is there — and the C6's table does move
//! (the 2026-10 repartition), so there must be one copy of the layout: the
//! table espflash writes. `esp-bootloader-esp-idf` is already a dependency
//! (`esp_app_desc!`).

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
    /// The table is read into a **heap** buffer (3 KB, freed on return), not
    /// a stack array: the C6's static RAM comes out of the main stack, and
    /// this runs once at boot when the heap has room.
    ///
    /// Returns `None` when the table has no such entry — which means the image
    /// was flashed without `--partition-table lp-fw/fw-esp32c6/partitions.csv`
    /// and espflash substituted its default table (or the table is corrupt).
    /// That is a flashing mistake, not a runtime condition, so the caller says
    /// so loudly and boots on memory FS rather than guess an offset.
    pub fn locate(flash: &mut esp_storage::FlashStorage<'static>) -> Option<Self> {
        // Word-backed so the buffer is 4-byte aligned for the NorFlash read
        // path below.
        let mut words = alloc::vec![0u32; partitions::PARTITION_TABLE_MAX_LEN / 4];
        let buf: &mut [u8] = words_as_bytes_mut(&mut words);
        let mut reader = AlignedTableReader(flash);
        let table = partitions::read_partition_table(&mut reader, buf).ok()?;
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

/// `read_partition_table` reads through `embedded_storage::ReadStorage`,
/// whose esp-storage implementation stages every read through a 4 KB
/// **stack** sector buffer — measured as +2,992 B of main-task stack
/// high-water on the C6's heap ratchet. The `NorFlash` read path reads
/// straight into a word-aligned buffer instead, so this adapter routes the
/// table read through it (the buffer [`LpfsPartition::locate`] passes is
/// word-backed; an unaligned one would panic under `panic-unaligned-buffer`,
/// loudly). Read-only: a write is refused.
struct AlignedTableReader<'a>(&'a mut esp_storage::FlashStorage<'static>);

impl embedded_storage::ReadStorage for AlignedTableReader<'_> {
    type Error = esp_storage::FlashStorageError;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        ReadNorFlash::read(self.0, offset, bytes)
    }

    fn capacity(&self) -> usize {
        ReadNorFlash::capacity(self.0)
    }
}

impl embedded_storage::Storage for AlignedTableReader<'_> {
    fn write(&mut self, _offset: u32, _bytes: &[u8]) -> Result<(), Self::Error> {
        Err(esp_storage::FlashStorageError::IoError)
    }
}

/// View a `u32` buffer as its bytes (alignment 4, length × 4).
fn words_as_bytes_mut(words: &mut [u32]) -> &mut [u8] {
    // SAFETY: u8 has no alignment or validity requirements; the slice covers
    // exactly the words' storage and borrows it mutably for its lifetime.
    unsafe { core::slice::from_raw_parts_mut(words.as_mut_ptr().cast::<u8>(), words.len() * 4) }
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
    let mut config = Config::new(BLOCK_SIZE, LPFS_BLOCK_COUNT.load(Ordering::Relaxed));
    config.cache_size = CACHE_SIZE;
    config.lookahead_size = LOOKAHEAD_SIZE;
    config
}
