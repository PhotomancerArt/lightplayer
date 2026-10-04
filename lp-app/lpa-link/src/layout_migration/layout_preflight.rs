//! Would writing this image's partition table move a board's files?
//!
//! Every flash path that is not Studio's Update (which migrates) asks this
//! first (plan MQ7): `lp-cli fwcheck`, `lp-cli validate run`,
//! `lp-cli hardware lpfs`, and `just flash-fw-esp32c6`. **Both directions**
//! refuse: a pre-repartition image written onto a migrated board would mount
//! its filesystem at `0x310000` — across the new app tail and the new
//! `lpfs` — fail, and format, destroying the migrated files. That downgrade
//! is exactly what flashing a pinned reference image does.

use crate::provider::partition_table::{PartitionTable, PartitionTableError};

use super::legacy_layout::is_legacy_c6_v1;
use super::lpfs_geometry::LpfsGeometry;

/// The answer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LayoutPreflight {
    /// The board already holds this layout: its files stay where they are.
    Same,
    /// The board holds a different LightPlayer layout: writing would strand
    /// (or destroy) its files.
    Differs { from: String, to: String },
    /// A blank chip: nothing to lose.
    DeviceBlank,
    /// A table that is not a LightPlayer layout (no `lpfs` row, or one that
    /// does not parse): somebody else's firmware.
    DeviceForeign,
}

impl LayoutPreflight {
    /// May a plain flash proceed without `--migrate` / `--discard-lpfs`?
    pub fn allows_plain_flash(&self) -> bool {
        !matches!(self, Self::Differs { .. })
    }
}

/// Compare the board's table (raw bytes read at `0x8000`) with the image's.
pub fn preflight(device_table: &[u8], image_table: &PartitionTable) -> LayoutPreflight {
    let device = match PartitionTable::parse(device_table) {
        Ok(table) => table,
        Err(PartitionTableError::Blank) => return LayoutPreflight::DeviceBlank,
        Err(_) => return LayoutPreflight::DeviceForeign,
    };
    if device.find("lpfs").is_none() {
        return LayoutPreflight::DeviceForeign;
    }
    if device.same_layout(image_table) {
        return LayoutPreflight::Same;
    }
    LayoutPreflight::Differs {
        from: describe_layout(&device),
        to: describe_layout(image_table),
    }
}

/// A layout in words: `"pre-2026-10 C6 layout (files at 0x310000, 960 KB)"`.
pub fn describe_layout(table: &PartitionTable) -> String {
    let files = match LpfsGeometry::from_table(table) {
        Some(geometry) => format!(
            "files at {:#x}, {} KB",
            geometry.offset,
            geometry.len() / 1024
        ),
        None => "no filesystem partition".to_string(),
    };
    if is_legacy_c6_v1(table) {
        format!("pre-2026-10 C6 layout ({files})")
    } else {
        format!("layout with {files}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout_migration::legacy_layout::legacy_c6_v1_table;

    fn d1() -> PartitionTable {
        let mut entries = legacy_c6_v1_table().entries().to_vec();
        entries[3].size = 0x34_0000;
        entries[4].offset = 0x35_0000;
        entries[4].size = 0xB_0000;
        PartitionTable::new(entries)
    }

    #[test]
    fn every_combination() {
        let legacy = legacy_c6_v1_table();
        assert_eq!(preflight(&d1().to_bytes(), &d1()), LayoutPreflight::Same);
        assert!(matches!(
            preflight(&legacy.to_bytes(), &d1()),
            LayoutPreflight::Differs { .. }
        ));
        // The downgrade: a pre-repartition image onto a migrated board.
        let downgrade = preflight(&d1().to_bytes(), &legacy);
        assert!(!downgrade.allows_plain_flash());
        if let LayoutPreflight::Differs { from, to } = downgrade {
            assert!(from.contains("0x350000"), "{from}");
            assert!(to.contains("pre-2026-10"), "{to}");
        }
        assert_eq!(
            preflight(&[0xFF; 0xC00], &d1()),
            LayoutPreflight::DeviceBlank
        );
        assert_eq!(
            preflight(&[0x12; 0xC00], &d1()),
            LayoutPreflight::DeviceForeign
        );
        let mut no_lpfs = legacy.entries().to_vec();
        no_lpfs.pop();
        assert_eq!(
            preflight(&PartitionTable::new(no_lpfs).to_bytes(), &d1()),
            LayoutPreflight::DeviceForeign
        );
        assert!(LayoutPreflight::DeviceBlank.allows_plain_flash());
    }
}
