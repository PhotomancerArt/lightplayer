//! `--flash-cut <spec>` on the C6: the spec and its parser live in
//! `lp_emu_esp_common::engine::flash_cut_spec` (moved there for the S3, S3
//! radio M7 P5) and are re-exported here unchanged; what stays is the one
//! chip fact the spec needs — the default range, the C6's own `lpfs` row.

use core::ops::Range;

use lp_emu_esp_common::engine::flash_cut::FlashCut;
pub use lp_emu_esp_common::engine::flash_cut_spec::{AfterCut, FLASH_CUT_MARKER, FlashCutSpec};

use crate::flash::{LPFS_LEN, LPFS_OFFSET};

/// A spec's plan on the C6: over its own range, or the `lpfs` row of
/// `chip`'s partition table ([`lpfs_range`]). A trait so the C6's call
/// sites read `spec.plan(chip)` as they did before the move.
pub trait FlashCutPlan {
    fn plan(&self, chip: &[u8]) -> FlashCut;
}

impl FlashCutPlan for FlashCutSpec {
    fn plan(&self, chip: &[u8]) -> FlashCut {
        self.plan_in(lpfs_range(chip))
    }
}

/// The `lpfs` partition as `chip`'s own table says, or the shipped table's
/// row when no table parses or none is called `lpfs`.
pub fn lpfs_range(chip: &[u8]) -> Range<u32> {
    crate::image::partitions(chip)
        .into_iter()
        .find(|p| p.label == "lpfs")
        .map(|p| p.offset..p.offset.saturating_add(p.len))
        .unwrap_or(LPFS_OFFSET..LPFS_OFFSET + LPFS_LEN)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::engine::flash_cut::TearModel;

    #[test]
    fn the_default_range_is_the_chips_own_lpfs_row() {
        // No table: the shipped constants.
        let blank = vec![0xffu8; 0x10000];
        assert_eq!(lpfs_range(&blank), LPFS_OFFSET..LPFS_OFFSET + LPFS_LEN);
        // The shipped table: the same row.
        let mut chip = vec![0xffu8; 0x10000];
        let table = crate::flash::c6_partition_table_bytes();
        let at = crate::flash::PARTITION_TABLE_OFFSET as usize;
        chip[at..at + table.len()].copy_from_slice(&table);
        assert_eq!(lpfs_range(&chip), LPFS_OFFSET..LPFS_OFFSET + LPFS_LEN);
        // A table whose lpfs is elsewhere (the fielded 176-sector one): that row.
        let row = (at..at + table.len())
            .step_by(32)
            .find(|&r| &chip[r + 12..r + 16] == b"lpfs")
            .expect("the lpfs row");
        chip[row + 4..row + 8].copy_from_slice(&0x0035_0000u32.to_le_bytes());
        chip[row + 8..row + 12].copy_from_slice(&0x000B_0000u32.to_le_bytes());
        assert_eq!(lpfs_range(&chip), 0x35_0000..0x40_0000);
        let spec = FlashCutSpec::new(3, TearModel::Clean, 1);
        assert_eq!(spec.plan(&chip).range, 0x35_0000..0x40_0000);
    }
}
