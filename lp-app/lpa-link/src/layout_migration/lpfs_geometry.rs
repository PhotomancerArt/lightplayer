//! Where a LightPlayer filesystem is, and the littlefs configuration the
//! firmware mounts it with.
//!
//! The geometry must equal the firmware's `lpfs_config()`
//! (`lp-fw/fw-esp32c6/src/flash_storage.rs`): 4096 B blocks, 512 B cache,
//! 64 B lookahead, the block count from the partition table (plan Q15). A
//! re-packed image built with any other cache or lookahead would still be
//! littlefs — but this module only ever builds what the firmware mounts.

use littlefs_rust::Config;

use crate::provider::partition_table::PartitionTable;

/// littlefs block size: one 4 KB flash sector.
pub const LPFS_BLOCK_SIZE: u32 = 4096;
/// The firmware's read/program cache.
pub const LPFS_CACHE_SIZE: u32 = 512;
/// The firmware's lookahead bitmap.
pub const LPFS_LOOKAHEAD_SIZE: u32 = 64;

/// A littlefs region of flash: its first byte and how many blocks it holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct LpfsGeometry {
    pub offset: u32,
    pub block_count: u32,
}

impl LpfsGeometry {
    /// The `lpfs` row of `table`, or `None` when it has none.
    pub fn from_table(table: &PartitionTable) -> Option<Self> {
        let row = table.find("lpfs")?;
        Some(Self {
            offset: row.offset,
            block_count: row.size / LPFS_BLOCK_SIZE,
        })
    }

    /// Bytes in the region.
    pub fn len(&self) -> u32 {
        self.block_count * LPFS_BLOCK_SIZE
    }

    /// Whether the region is empty (never, for a real partition).
    pub fn is_empty(&self) -> bool {
        self.block_count == 0
    }

    /// One past the last byte.
    pub fn end(&self) -> u32 {
        self.offset + self.len()
    }

    /// The firmware's littlefs configuration for this region.
    pub fn config(&self) -> Config {
        let mut config = Config::new(LPFS_BLOCK_SIZE, self.block_count);
        config.cache_size = LPFS_CACHE_SIZE;
        config.lookahead_size = LPFS_LOOKAHEAD_SIZE;
        config
    }
}

/// Does a littlefs superblock sit at the start of `region_start`?
///
/// littlefs keeps its superblock in a metadata pair, blocks 0 and 1, each
/// beginning with a 4-byte revision count, a tag, and the magic `littlefs`
/// at byte 8. A cheap, read-only "is there a filesystem here?" for the
/// layout probe: it decides what to read next, never whether files are
/// intact (a full mount does that).
pub fn has_superblock(region_start: &[u8]) -> bool {
    const MAGIC: &[u8; 8] = b"littlefs";
    let block = LPFS_BLOCK_SIZE as usize;
    [0usize, block]
        .iter()
        .any(|&at| region_start.get(at + 8..at + 16) == Some(&MAGIC[..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use littlefs_rust::{Filesystem, RamStorage};

    /// The firmware's numbers, pinned: a change here is a change to what
    /// every re-packed board mounts.
    #[test]
    fn the_geometry_is_the_firmwares() {
        assert_eq!(LPFS_BLOCK_SIZE, 4096);
        assert_eq!(LPFS_CACHE_SIZE, 512);
        assert_eq!(LPFS_LOOKAHEAD_SIZE, 64);
        let csv = include_str!("../../../../lp-fw/fw-esp32c6/src/flash_storage.rs");
        assert!(csv.contains("const BLOCK_SIZE: u32 = 4096;"));
        assert!(csv.contains("const CACHE_SIZE: u32 = 512;"));
        assert!(csv.contains("const LOOKAHEAD_SIZE: u32 = 64;"));
    }

    #[test]
    fn a_formatted_region_has_a_superblock_and_erased_flash_does_not() {
        let geometry = LpfsGeometry {
            offset: 0,
            block_count: 32,
        };
        let mut storage = RamStorage::new(LPFS_BLOCK_SIZE, 32);
        assert!(!has_superblock(storage.data()));
        Filesystem::format(&mut storage, &geometry.config()).unwrap();
        assert!(has_superblock(storage.data()));
    }
}
