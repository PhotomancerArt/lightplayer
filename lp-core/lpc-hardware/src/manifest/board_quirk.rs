//! Compiled-in board quirks: what a board needs from firmware before it works.
//!
//! A dev board is a chip plus decisions its maker took, and some of those
//! decisions need the firmware's cooperation. The XIAO ESP32-C6 puts an RF
//! switch between the chip and its antennas that is dead until a GPIO powers
//! it (docs/defects/2026-09-23-xiao-c6-rf-switch-never-powered.md).
//!
//! The table is keyed on the board id of the manifest **in effect** —
//! [`HwManifest::board_id`](crate::HwManifest::board_id), which includes a
//! firmware's compiled-in fallback — and deliberately lives in code, not in
//! `hardware.json`: the manifest schema is versioned under `schemas/history/`,
//! and a new field there would be a persisted-format change. A board id with
//! no entry gets nothing, which is the point: the same pins are ordinary GPIO
//! on other boards and must not be driven blindly.
//!
//! This module is the pure half, free of any HAL type so host tests can reach
//! it. The firmware applies it (`fw-esp32c6`'s `board_quirks.rs`).

use crate::HwGateLevel;

/// The XIAO ESP32-C6's board id, as its checked-in manifest spells it.
pub const XIAO_ESP32_C6_BOARD_ID: &str = "seeed/xiao-esp32-c6";

/// One thing a board needs done at boot, named for the boot log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardQuirk {
    /// Short stable name, printed in the boot log line that applies it.
    pub name: &'static str,
    /// Pins to drive at boot, before any radio init, and hold for the life
    /// of the program.
    pub gpio_holds: &'static [GpioHold],
}

/// Drive `gpio` to `level` and keep it there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpioHold {
    pub gpio: u8,
    pub level: HwGateLevel,
}

/// The quirks the board `board_id` needs; empty for any board without one.
///
/// The match is exact: a board id is an identifier, not a pattern.
pub fn board_quirks_for(board_id: &str) -> &'static [BoardQuirk] {
    match board_id {
        XIAO_ESP32_C6_BOARD_ID => &[XIAO_C6_RF_SWITCH],
        _ => &[],
    }
}

/// The XIAO ESP32-C6's FM8625H RF switch (Seeed wiki): GPIO3 LOW powers the
/// switch, GPIO14 selects the antenna — LOW the on-board ceramic one, HIGH
/// the U.FL connector. The ceramic antenna is what ships; choosing U.FL is
/// future work (a user setting), not a quirk.
const XIAO_C6_RF_SWITCH: BoardQuirk = BoardQuirk {
    name: "xiao-c6-rf-switch",
    gpio_holds: &[
        GpioHold {
            gpio: 3,
            level: HwGateLevel::Low,
        },
        GpioHold {
            gpio: 14,
            level: HwGateLevel::Low,
        },
    ],
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        default_desktop_hardware_manifest, default_esp32c6_hardware_manifest,
        default_esp32s3_hardware_manifest, default_esp32v3_hardware_manifest,
    };

    #[test]
    fn xiao_esp32_c6_powers_its_rf_switch_on_the_ceramic_antenna() {
        let quirks = board_quirks_for("seeed/xiao-esp32-c6");

        assert_eq!(quirks.len(), 1);
        assert_eq!(quirks[0].name, "xiao-c6-rf-switch");
        assert_eq!(
            quirks[0].gpio_holds,
            &[
                GpioHold {
                    gpio: 3,
                    level: HwGateLevel::Low,
                },
                GpioHold {
                    gpio: 14,
                    level: HwGateLevel::Low,
                },
            ]
        );
    }

    #[test]
    fn the_c6_fallback_manifest_is_the_xiao_so_it_takes_the_quirk() {
        // A C6 with no `/hardware.json` runs this manifest; the quirk keys on
        // the manifest in effect, fallback included.
        let manifest = default_esp32c6_hardware_manifest();

        assert_eq!(manifest.board_id(), XIAO_ESP32_C6_BOARD_ID);
        assert_eq!(board_quirks_for(manifest.board_id()), &[XIAO_C6_RF_SWITCH]);
    }

    #[test]
    fn every_other_compiled_in_board_takes_no_quirk() {
        // `permissive_emu_hardware_manifest` is absent on purpose: it is the
        // XIAO's pin map under the XIAO's id, for `fw-emu`, which applies no
        // board quirks.
        for manifest in [
            default_esp32s3_hardware_manifest(),
            default_esp32v3_hardware_manifest(),
            default_desktop_hardware_manifest(),
        ] {
            assert!(
                board_quirks_for(manifest.board_id()).is_empty(),
                "{} must take no board quirk",
                manifest.board_id()
            );
        }
    }

    #[test]
    fn every_other_checked_in_board_file_takes_no_quirk() {
        // Board files with no compiled-in fallback, the C6 DevKitC-1 among
        // them: GPIO3 and GPIO14 are ordinary pins there.
        for json in [
            include_str!("../../boards/espressif/esp32-c6-devkitc-1.json"),
            include_str!("../../boards/espressif/esp32-s3-devkitc-1.json"),
            include_str!("../../boards/quinled/dig2go.json"),
        ] {
            let manifest = crate::HardwareManifestFile::read_json(json)
                .and_then(|file| file.to_manifest())
                .expect("checked-in board manifest must parse");
            assert!(
                board_quirks_for(manifest.board_id()).is_empty(),
                "{} must take no board quirk",
                manifest.board_id()
            );
        }
    }

    #[test]
    fn a_near_miss_board_id_takes_no_quirk() {
        for board_id in [
            "",
            "seeed/xiao-esp32-c6 ",
            "SEEED/XIAO-ESP32-C6",
            "seeed/xiao-esp32-c6-plus",
            "xiao-esp32-c6",
            "seeed/xiao-esp32-s3-plus",
            "espressif/esp32-c6-devkitc-1",
        ] {
            assert!(
                board_quirks_for(board_id).is_empty(),
                "{board_id:?} must take no board quirk"
            );
        }
    }
}
