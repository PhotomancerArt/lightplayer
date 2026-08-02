//! Raw flash regions a link operation can address, per chip.
//!
//! A raw read is meaningless without an offset and a length, and those are
//! **per board**: the C6's `lpfs` sits at `0x310000` for 960 KB, the S3's at
//! `0x610000` for 1.5 MB (its 8 MB partition floor —
//! `docs/adr/2026-07-30-esp32s3-partition-floor.md`). Hardcoding the C6's
//! numbers would silently read the wrong 960 KB off an S3 and hand the user
//! a "backup" of somebody else's partition.
//!
//! **The chip is discovered, not declared.** A device that cannot boot cannot
//! tell Studio which board it is — that is the whole recovery scenario (see
//! the M5 plan correction in the recovery plan's notes). What *can* answer is
//! the esptool SYNC handshake both providers already perform before any flash
//! operation, so the region is resolved from the chip name that handshake
//! returns, at the moment of the read.
//!
//! The names arrive in two shapes: espflash's `Chip` renders `esp32c6`, while
//! esptool-js reports something like `ESP32-C6 (QFN32) (revision v0.2)`.
//! [`LinkFlashRegion::lpfs_for_chip`] normalizes both.

use serde::{Deserialize, Serialize};

/// A contiguous span of device flash, in bytes from the start of the chip.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct LinkFlashRegion {
    pub offset: u32,
    pub length: u32,
}

impl LinkFlashRegion {
    /// The `lpfs` partition on `chip_name`, or `None` for a chip this build
    /// has no partition table for.
    ///
    /// Returning `None` rather than guessing is deliberate: a wrong region
    /// produces a plausible-looking archive of the wrong bytes, which is
    /// worse than a refusal in exactly the situation where the user is
    /// trying to rescue their work.
    pub fn lpfs_for_chip(chip_name: &str) -> Option<Self> {
        let key = normalize_chip_name(chip_name);
        LPFS_PARTITIONS
            .iter()
            .find(|(chip, _)| key.contains(chip))
            .map(|(_, region)| *region)
    }

    /// Block count for a littlefs mount over this region at `block_size`.
    pub fn block_count(&self, block_size: u32) -> u32 {
        self.length / block_size
    }
}

/// The `lpfs` partition of every board LightPlayer ships a partition table
/// for. Guarded against the tables themselves by the tests below.
///
/// Keys are matched by substring against the normalized chip name, so a new
/// entry must not be a substring of another one (`esp32` alone would swallow
/// every board); today's keys are disjoint.
const LPFS_PARTITIONS: &[(&str, LinkFlashRegion)] = &[
    (
        "esp32c6",
        LinkFlashRegion {
            offset: 0x0031_0000,
            length: 0x000F_0000,
        },
    ),
    (
        "esp32s3",
        LinkFlashRegion {
            offset: 0x0061_0000,
            length: 0x0018_0000,
        },
    ),
];

/// Reduce a reported chip name to lowercase alphanumerics so `esp32c6`,
/// `ESP32-C6 (QFN32) (revision v0.2)` and `ESP32-C6` all answer the same.
fn normalize_chip_name(chip_name: &str) -> String {
    chip_name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hand-maintained agreement between this table and the firmware
    /// partition tables — the same guard `lp-bootctl` keeps over its sector
    /// offset, for the same reason: nothing else would notice the drift, and
    /// the failure mode is a silently wrong backup.
    #[test]
    fn lpfs_regions_match_every_boards_partition_table() {
        for (board, csv) in [
            (
                "esp32c6",
                include_str!("../../../../lp-fw/fw-esp32c6/partitions.csv"),
            ),
            (
                "esp32s3",
                include_str!("../../../../lp-fw/fw-esp32s3/partitions.csv"),
            ),
        ] {
            let (offset, size) = lpfs_row(csv);
            let region = LinkFlashRegion::lpfs_for_chip(board)
                .unwrap_or_else(|| panic!("{board} has an lpfs region"));
            assert_eq!(region.offset, offset, "{board}: lpfs offset drifted");
            assert_eq!(region.length, size, "{board}: lpfs size drifted");
        }
    }

    #[test]
    fn chip_names_normalize_across_both_reporters() {
        // espflash's `Chip` Display, and esptool-js's chatty banner.
        let expected = LinkFlashRegion::lpfs_for_chip("esp32c6").unwrap();
        for reported in [
            "esp32c6",
            "ESP32-C6",
            "ESP32-C6 (QFN32) (revision v0.2)",
            "esp32-c6",
        ] {
            assert_eq!(
                LinkFlashRegion::lpfs_for_chip(reported),
                Some(expected),
                "{reported} should resolve to the C6 lpfs region"
            );
        }
    }

    #[test]
    fn an_unknown_chip_refuses_rather_than_guessing() {
        assert_eq!(LinkFlashRegion::lpfs_for_chip("ESP32-C3"), None);
        assert_eq!(LinkFlashRegion::lpfs_for_chip(""), None);
    }

    #[test]
    fn block_count_covers_the_whole_region() {
        let c6 = LinkFlashRegion::lpfs_for_chip("esp32c6").unwrap();
        assert_eq!(c6.block_count(4096), 240);
        let s3 = LinkFlashRegion::lpfs_for_chip("esp32s3").unwrap();
        assert_eq!(s3.block_count(4096), 384);
    }

    fn lpfs_row(csv: &str) -> (u32, u32) {
        csv.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| line.split(',').map(str::trim).collect::<Vec<_>>())
            .find(|fields| fields.first() == Some(&"lpfs"))
            .map(|fields| (parse_hex(fields[3]), parse_hex(fields[4])))
            .expect("partitions.csv declares lpfs")
    }

    fn parse_hex(text: &str) -> u32 {
        let digits = text
            .strip_prefix("0x")
            .or_else(|| text.strip_prefix("0X"))
            .unwrap_or(text);
        u32::from_str_radix(digits, 16).unwrap_or_else(|_| panic!("{text:?} is not hex"))
    }
}
