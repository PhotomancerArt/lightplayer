//! Where the flash partitions are: `lpfs` (the tree store, `tree_flash`)
//! and, for a split image, `factory`.
//!
//! ## Why no offset constant
//!
//! A transcribed `lpfs` offset would be actively dangerous: the S3's 8 MB
//! table (M3 P1) puts `lpfs` at `0x610000`, so a constant copied from another
//! chip's table points into the middle of this chip's **factory** partition —
//! a filesystem mount would silently erase running code, with no diagnostic.
//! (The C6 transcribed its offset until 2026-10; all three chips now read
//! their table — `docs/debt/firmware-partition-constants-transcribed.md`.)
//!
//! So the offset and length are read from the partition table at runtime,
//! matched by the `lpfs` label. `esp-bootloader-esp-idf` is already a
//! dependency (`esp_app_desc!`), the table is the same artifact espflash
//! writes, and there is no second copy of the layout to keep in sync.

use esp_bootloader_esp_idf::partitions;

/// Partition label to mount as the LightPlayer filesystem. Must match
/// `partitions.csv`.
const LPFS_LABEL: &str = "lpfs";
/// The app partition: a split image's loader, records, core and engine.
#[cfg(lp_split)]
const FACTORY_LABEL: &str = "factory";

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

    /// The `factory` partition's `(offset, len)`, by label: where a split
    /// image's region ends (the C6 reads the same row, `flash_layout.rs`).
    /// `None` when the table has none.
    #[cfg(lp_split)]
    pub fn locate_factory(flash: &mut impl embedded_storage::Storage) -> Option<(u32, u32)> {
        let mut buf = [0u8; partitions::PARTITION_TABLE_MAX_LEN];
        let table = partitions::read_partition_table(flash, &mut buf).ok()?;
        let entry = table.iter().find(|e| e.label_as_str() == FACTORY_LABEL)?;
        Some((entry.offset(), entry.len()))
    }

    /// The partition's first byte.
    pub fn offset(&self) -> u32 {
        self.offset
    }

    /// Its length in bytes.
    pub fn len(&self) -> u32 {
        self.len
    }
}
