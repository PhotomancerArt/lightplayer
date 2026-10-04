//! The ESP32-C6 layout every board flashed before the 2026-10 repartition
//! holds — frozen.
//!
//! A historical fact, not a mirror of `partitions.csv`: it never changes,
//! and only the migration reads it. `testdata/partitions-esp32c6-legacy-v1.csv`
//! is the same table as text, held equal to these rows by a test.
//!
//! Recognizing **LightPlayer's** tables matters: a board running somebody
//! else's firmware (MicroPython, the factory demo) has a table too, and its
//! data partition must never be read as a LightPlayer filesystem and
//! "migrated". Anything that is neither the target table nor this one is
//! `Foreign`.

use crate::provider::partition_table::{PartitionEntry, PartitionTable};

use super::lpfs_geometry::LpfsGeometry;

/// The pre-repartition C6 `lpfs`: `0x310000`, 240 blocks (960 KB).
pub const LEGACY_C6_V1_LPFS: LpfsGeometry = LpfsGeometry {
    offset: 0x0031_0000,
    block_count: 240,
};

/// The frozen pre-repartition C6 table.
pub fn legacy_c6_v1_table() -> PartitionTable {
    let row = |label: &str, kind: u8, subtype: u8, offset: u32, size: u32| PartitionEntry {
        label: label.to_string(),
        kind,
        subtype,
        offset,
        size,
        flags: 0,
    };
    PartitionTable::new(vec![
        row("nvs", 0x01, 0x02, 0x9000, 0x5000),
        row("bootctl", 0x01, 0x06, 0xe000, 0x1000),
        row("phy_init", 0x01, 0x01, 0xf000, 0x1000),
        row("factory", 0x00, 0x00, 0x10000, 0x30_0000),
        row(
            "lpfs",
            0x01,
            0x82,
            LEGACY_C6_V1_LPFS.offset,
            LEGACY_C6_V1_LPFS.len(),
        ),
    ])
}

/// Is `table` the frozen pre-repartition C6 layout?
pub fn is_legacy_c6_v1(table: &PartitionTable) -> bool {
    table.same_layout(&legacy_c6_v1_table())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frozen_rows_are_the_committed_fixture() {
        let csv = include_str!("../../testdata/partitions-esp32c6-legacy-v1.csv");
        let fixture = PartitionTable::from_csv(csv).unwrap();
        assert!(is_legacy_c6_v1(&fixture));
        assert_eq!(fixture, legacy_c6_v1_table());
    }

    #[test]
    fn the_legacy_lpfs_is_the_tables_lpfs_row() {
        assert_eq!(
            LpfsGeometry::from_table(&legacy_c6_v1_table()),
            Some(LEGACY_C6_V1_LPFS)
        );
        assert_eq!(LEGACY_C6_V1_LPFS.end(), 0x40_0000);
    }

    #[test]
    fn the_s3_table_is_not_the_legacy_c6_table() {
        let s3 =
            PartitionTable::from_csv(include_str!("../../../../lp-fw/fw-esp32s3/partitions.csv"))
                .unwrap();
        assert!(!is_legacy_c6_v1(&s3));
    }
}
