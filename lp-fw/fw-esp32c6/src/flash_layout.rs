//! Where `factory` and `lpfs` are, read once from the flashed partition
//! table.
//!
//! The offsets come from the table the chip was flashed with, matched by
//! label — the same pattern as `fw-esp32s3` and `fw-esp32v3`. Until 2026-10
//! the C6 hardcoded `0x310000` / 240 blocks, transcribed by hand from
//! `partitions.csv` (`docs/debt/firmware-partition-constants-transcribed.md`,
//! paid). A transcribed offset that drifts from the flashed table does not
//! fail, it mounts across whatever is there — and the C6's table does move
//! (the 2026-10 repartition), so there is one copy of the layout: the table
//! espflash writes.
//!
//! One read serves both readers: the filesystem mounts `lpfs`, and a split
//! image's boot bookkeeping takes the region's end from `factory`'s
//! (`lp_bootctl::SplitLayout::from_factory`). It runs right after
//! `FlashStorage::new`, before anything else touches flash.

use esp_bootloader_esp_idf::partitions;

/// Partition label to mount as the LightPlayer filesystem. Must match
/// `partitions.csv`.
const LPFS_LABEL: &str = "lpfs";
/// The app partition the IDF bootloader boots.
#[cfg(lp_split)]
const FACTORY_LABEL: &str = "factory";

/// A partition's place in flash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartitionExtent {
    pub offset: u32,
    pub len: u32,
}

/// The two partitions the firmware needs, as the flashed table says. Either
/// is `None` when the table could not be read or has no such entry — which
/// means the image was flashed without `--partition-table
/// lp-fw/fw-esp32c6/partitions.csv` and espflash substituted its default
/// table (or the table is corrupt).
#[derive(Debug, Clone, Copy, Default)]
pub struct FlashLayout {
    /// Read only by a split image's boot bookkeeping.
    #[cfg(lp_split)]
    pub factory: Option<PartitionExtent>,
    pub lpfs: Option<PartitionExtent>,
}

impl FlashLayout {
    /// Read the table once.
    ///
    /// The table is read into a **heap** buffer (3 KB, freed on return), not
    /// a stack array: the C6's static RAM comes out of the main stack, and
    /// this runs once at boot when the heap has room.
    pub fn locate(flash: &mut esp_storage::FlashStorage<'static>) -> Self {
        // Word-backed so the buffer is 4-byte aligned for the NorFlash read
        // path below.
        let mut words = alloc::vec![0u32; partitions::PARTITION_TABLE_MAX_LEN / 4];
        let buf: &mut [u8] = words_as_bytes_mut(&mut words);
        let mut reader = AlignedTableReader(flash);
        let Ok(table) = partitions::read_partition_table(&mut reader, buf) else {
            return Self::default();
        };
        let find = |label: &str| {
            table
                .iter()
                .find(|e| e.label_as_str() == label)
                .map(|e| PartitionExtent {
                    offset: e.offset(),
                    len: e.len(),
                })
        };
        Self {
            #[cfg(lp_split)]
            factory: find(FACTORY_LABEL),
            lpfs: find(LPFS_LABEL),
        }
    }
}

/// `read_partition_table` reads through `embedded_storage::ReadStorage`,
/// whose esp-storage implementation stages every read through a 4 KB
/// **stack** sector buffer — measured as +2,992 B of main-task stack
/// high-water on the C6's heap ratchet. The `NorFlash` read path reads
/// straight into a word-aligned buffer instead, so this adapter routes the
/// table read through it (the buffer [`FlashLayout::locate`] passes is
/// word-backed; an unaligned one would panic under `panic-unaligned-buffer`,
/// loudly). Read-only: a write is refused.
struct AlignedTableReader<'a>(&'a mut esp_storage::FlashStorage<'static>);

impl embedded_storage::ReadStorage for AlignedTableReader<'_> {
    type Error = esp_storage::FlashStorageError;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        embedded_storage::nor_flash::ReadNorFlash::read(self.0, offset, bytes)
    }

    fn capacity(&self) -> usize {
        embedded_storage::nor_flash::ReadNorFlash::capacity(self.0)
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
