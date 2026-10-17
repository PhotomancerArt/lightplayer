//! The ESP32-C6 layouts boards in the field hold — frozen.
//!
//! Historical facts, not mirrors of `partitions.csv`: they never change, and
//! only the migrations read them. Each has a `testdata/` CSV holding the
//! same table as text, held equal to these rows by a test.
//!
//! | layout | `factory` | `lpfs` | filesystem | fielded |
//! |---|---|---|---|---|
//! | v1 ([`legacy_c6_v1_table`]) | `0x10000`+3 MB | `0x310000`, 240 blocks | littlefs | before the 2026-10 repartition |
//! | v2 ([`legacy_c6_v2_table`]) | `0x10000`+3.25 MB | `0x350000`, 176 blocks | littlefs | from the repartition to the tree-store cutover |
//!
//! Recognizing **LightPlayer's** tables matters: a board running somebody
//! else's firmware (MicroPython, the factory demo) has a table too, and its
//! data partition must never be read as a LightPlayer filesystem and
//! "migrated". Anything that is neither the target table nor one of these
//! is `Foreign`.

use crate::provider::partition_table::{PartitionEntry, PartitionTable};

use super::lpfs_geometry::LpfsGeometry;

/// The pre-repartition C6 `lpfs`: `0x310000`, 240 blocks (960 KB).
pub const LEGACY_C6_V1_LPFS: LpfsGeometry = LpfsGeometry {
    offset: 0x0031_0000,
    block_count: 240,
};

/// The 2026-10 repartition's C6 `lpfs`: `0x350000`, 176 blocks (704 KB) —
/// littlefs on every board fielded before the tree-store cutover.
pub const LEGACY_C6_V2_LPFS: LpfsGeometry = LpfsGeometry {
    offset: 0x0035_0000,
    block_count: 176,
};

fn c6_table(factory_len: u32, lpfs: LpfsGeometry) -> PartitionTable {
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
        row("factory", 0x00, 0x00, 0x10000, factory_len),
        row("lpfs", 0x01, 0x82, lpfs.offset, lpfs.len()),
    ])
}

/// The frozen pre-repartition C6 table.
pub fn legacy_c6_v1_table() -> PartitionTable {
    c6_table(0x30_0000, LEGACY_C6_V1_LPFS)
}

/// The frozen 2026-10 repartition C6 table (`partitions.csv` until the
/// tree-store cutover).
pub fn legacy_c6_v2_table() -> PartitionTable {
    c6_table(0x34_0000, LEGACY_C6_V2_LPFS)
}

/// Is `table` the frozen pre-repartition C6 layout?
pub fn is_legacy_c6_v1(table: &PartitionTable) -> bool {
    table.same_layout(&legacy_c6_v1_table())
}

/// Is `table` the frozen 2026-10 repartition C6 layout?
pub fn is_legacy_c6_v2(table: &PartitionTable) -> bool {
    table.same_layout(&legacy_c6_v2_table())
}

/// The frozen C6 layout `table` is, if any: its `lpfs` geometry.
pub fn known_c6_layout(table: &PartitionTable) -> Option<LpfsGeometry> {
    if is_legacy_c6_v1(table) {
        Some(LEGACY_C6_V1_LPFS)
    } else if is_legacy_c6_v2(table) {
        Some(LEGACY_C6_V2_LPFS)
    } else {
        None
    }
}

/// Every frozen C6 `lpfs`, newest first: where an older layout's filesystem
/// may still be when a board's table already moved past it.
pub const KNOWN_C6_LPFS: [LpfsGeometry; 2] = [LEGACY_C6_V2_LPFS, LEGACY_C6_V1_LPFS];

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
    fn the_frozen_v2_rows_are_the_committed_fixture() {
        let csv = include_str!("../../testdata/partitions-esp32c6-legacy-v2.csv");
        let fixture = PartitionTable::from_csv(csv).unwrap();
        assert!(is_legacy_c6_v2(&fixture));
        assert_eq!(fixture, legacy_c6_v2_table());
        assert_eq!(known_c6_layout(&fixture), Some(LEGACY_C6_V2_LPFS));
    }

    /// Until the cutover's default switch, `partitions.csv` IS the v2 table;
    /// after it, it is the tree table. Either way it is never a third one
    /// without a frozen copy here.
    #[test]
    fn partitions_csv_is_the_v2_table_or_the_tree_table() {
        let current =
            PartitionTable::from_csv(include_str!("../../../../lp-fw/fw-esp32c6/partitions.csv"))
                .unwrap();
        let tree = PartitionTable::from_csv(include_str!(
            "../../../../lp-fw/fw-esp32c6/partitions-tree.csv"
        ))
        .unwrap();
        assert!(is_legacy_c6_v2(&current) || current.same_layout(&tree));
        assert_eq!(
            LpfsGeometry::from_table(&tree),
            Some(LpfsGeometry {
                offset: 0x38_0000,
                block_count: 128
            })
        );
        assert_eq!(tree.find("factory").map(|f| f.size), Some(0x37_0000));
        assert_eq!(known_c6_layout(&tree), None);
    }

    #[test]
    fn the_legacy_lpfs_is_the_tables_lpfs_row() {
        assert_eq!(
            LpfsGeometry::from_table(&legacy_c6_v1_table()),
            Some(LEGACY_C6_V1_LPFS)
        );
        assert_eq!(LEGACY_C6_V1_LPFS.end(), 0x40_0000);
        assert_eq!(LEGACY_C6_V2_LPFS.end(), 0x40_0000);
    }

    #[test]
    fn the_s3_table_is_not_the_legacy_c6_table() {
        let s3 =
            PartitionTable::from_csv(include_str!("../../../../lp-fw/fw-esp32s3/partitions.csv"))
                .unwrap();
        assert!(!is_legacy_c6_v1(&s3));
        assert_eq!(known_c6_layout(&s3), None);
    }
}
