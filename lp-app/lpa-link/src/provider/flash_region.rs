//! Raw flash regions a link operation can address.
//!
//! A raw read is meaningless without an offset and a length, and those are
//! **per board** — not even per chip since the 2026-10 C6 repartition: a C6
//! flashed before it keeps `lpfs` at `0x310000` for 960 KB, one flashed after
//! at `0x350000` for 704 KB, and the S3 at `0x610000`. A guessed region would
//! hand the user a plausible-looking "backup" of somebody else's bytes.
//!
//! **The region is read, not declared.** The answer is the device's own
//! partition table, read at `0x8000` in the same bootloader session as the
//! raw read ([`LinkFlashRegion::lpfs_in`]). A device that cannot boot cannot
//! say where its filesystem is; its table can. A table with no `lpfs` row is
//! not a LightPlayer layout, and the raw read refuses it rather than
//! guessing.

use serde::{Deserialize, Serialize};

use super::partition_table::PartitionTable;

/// A contiguous span of device flash, in bytes from the start of the chip.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct LinkFlashRegion {
    pub offset: u32,
    pub length: u32,
}

impl LinkFlashRegion {
    /// The `lpfs` partition `table` declares, or `None` when it declares
    /// none (not a LightPlayer layout).
    pub fn lpfs_in(table: &PartitionTable) -> Option<Self> {
        let row = table.find("lpfs")?;
        Some(Self {
            offset: row.offset,
            length: row.size,
        })
    }

    /// Block count for a littlefs mount over this region at `block_size`.
    pub fn block_count(&self, block_size: u32) -> u32 {
        self.length / block_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_region_is_each_shipped_tables_lpfs_row() {
        for (csv, offset, length, blocks) in [
            (
                include_str!("../../../../lp-fw/fw-esp32s3/partitions.csv"),
                0x0061_0000,
                0x0018_0000,
                384,
            ),
            (
                include_str!("../../testdata/partitions-esp32c6-legacy-v1.csv"),
                0x0031_0000,
                0x000F_0000,
                240,
            ),
        ] {
            let table = PartitionTable::from_csv(csv).unwrap();
            let region = LinkFlashRegion::lpfs_in(&table).unwrap();
            assert_eq!((region.offset, region.length), (offset, length));
            assert_eq!(region.block_count(4096), blocks);
        }
        let c6 =
            PartitionTable::from_csv(include_str!("../../../../lp-fw/fw-esp32c6/partitions.csv"))
                .unwrap();
        assert!(LinkFlashRegion::lpfs_in(&c6).is_some());
    }

    #[test]
    fn a_table_without_lpfs_refuses_rather_than_guessing() {
        let mut csv = String::new();
        csv.push_str("nvs, data, nvs, 0x9000, 0x6000,\n");
        csv.push_str("factory, app, factory, 0x10000, 0x100000,\n");
        let table = PartitionTable::from_csv(&csv).unwrap();
        assert_eq!(LinkFlashRegion::lpfs_in(&table), None);
    }
}
