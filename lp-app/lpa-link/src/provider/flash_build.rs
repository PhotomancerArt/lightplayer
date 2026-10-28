//! [`FlashBuild`]: which firmware package a flash's bootloader session
//! loads.
//!
//! The package used to be chosen before the port was opened, from the chip
//! Studio had heard passively — a boot banner, a hello's board id. A board
//! plugged in mid-stream, or one running a quiet firmware, never says, so
//! the user was asked to pick a board model just so a build could be named
//! (plan `lp2025/2026-10-28-0025-install-without-board-pick`).
//!
//! The bootloader session itself is the one place the chip is certain: its
//! SYNC names the silicon. [`FlashBuild::ByChip`] carries the app's table of
//! "this chip gets this build", and the session picks from it by the chip it
//! hears ([`FlashBuild::build_for_chip`]). [`FlashBuild::Fixed`] stays for a
//! caller that means one particular build (a release the user picked).
//! Either way the flash-time chip guard still checks the package against the
//! silicon before anything is written.

use serde::{Deserialize, Serialize};

use super::chip::chip_id_from_reported;

/// One row of a [`FlashBuild::ByChip`] table.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ChipBuild {
    /// A canonical chip id (`esp32c6`, `esp32s3`, `esp32`).
    pub chip: String,
    /// The build that chip gets (`lp-fw/builds/<id>.json`).
    pub build_id: String,
}

/// The package a bootloader session loads.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum FlashBuild {
    /// This build, whatever the chip says (the chip guard still refuses a
    /// mismatch).
    Fixed(String),
    /// The build the chip the session hears maps to, or none.
    ByChip(Vec<ChipBuild>),
}

impl FlashBuild {
    /// The build id for the chip the bootloader reported (any reporter's
    /// spelling), or `None` when this table has nothing for it. A `Fixed`
    /// build answers itself whatever the chip.
    pub fn build_for_chip(&self, reported_chip: &str) -> Option<&str> {
        match self {
            Self::Fixed(build_id) => Some(build_id),
            Self::ByChip(table) => {
                let chip = chip_id_from_reported(reported_chip)?;
                table
                    .iter()
                    .find(|row| chip_id_from_reported(&row.chip) == Some(chip))
                    .map(|row| row.build_id.as_str())
            }
        }
    }

    /// The build when it is known before the session (`Fixed`).
    pub fn fixed(&self) -> Option<&str> {
        match self {
            Self::Fixed(build_id) => Some(build_id),
            Self::ByChip(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> FlashBuild {
        FlashBuild::ByChip(vec![
            ChipBuild {
                chip: "esp32c6".into(),
                build_id: "esp32c6-4mb".into(),
            },
            ChipBuild {
                chip: "esp32".into(),
                build_id: "esp32v3-4mb".into(),
            },
        ])
    }

    #[test]
    fn a_table_answers_by_the_chip_the_bootloader_names() {
        assert_eq!(
            table().build_for_chip("ESP32-C6 (QFN32) (revision v0.2)"),
            Some("esp32c6-4mb")
        );
        assert_eq!(
            table().build_for_chip("ESP32-D0WD-V3 (revision v3.0)"),
            Some("esp32v3-4mb")
        );
    }

    #[test]
    fn a_chip_the_table_lacks_gets_nothing_not_the_nearest() {
        assert_eq!(table().build_for_chip("ESP32-S3 (QFN56) (revision v0.2)"), None);
        assert_eq!(table().build_for_chip("ESP8266"), None);
        assert_eq!(table().build_for_chip(""), None);
    }

    #[test]
    fn a_fixed_build_answers_itself() {
        let fixed = FlashBuild::Fixed("esp32s3-8mb".into());
        assert_eq!(fixed.build_for_chip("anything"), Some("esp32s3-8mb"));
        assert_eq!(fixed.fixed(), Some("esp32s3-8mb"));
        assert_eq!(table().fixed(), None);
    }
}
