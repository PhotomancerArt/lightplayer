//! Matching what a probe can see of a plugged-in board to the catalog.
//!
//! A flash needs two answers before it starts: which firmware build (the
//! chip decides that alone, [`builds_by_chip`]) and, only if it is worth
//! asking, which catalog board — the board's runtime manifest is what
//! `/hardware.json` gets. [`match_boards`] answers the second from facts the
//! host can read without a pick: the chip, the flash size from the flash id,
//! and the USB vendor of the port. It never guesses — when the facts leave
//! more than one board, it says so and lists them.
//!
//! The narrowing is a ladder of *hints*, not requirements. A board's catalog
//! flash size and bridge describe the usual unit; a particular unit can
//! differ (the C6 DevKitC-1's UART port is a CH343, though the catalog calls
//! its bridge native USB). So a step that would leave **zero** boards is
//! skipped rather than applied, and the answer stays the wider set.

use crate::catalog::{DESKTOP_FAMILY, all_boards};
use crate::display_manifest::BoardDisplayFile;
use crate::firmware_join::{all_builds, provisioning_build_id, served_build_ids};
use crate::usb_bridge::UsbBridge;

const MIB: u64 = 1024 * 1024;

/// What the host can observe of an attached board before anyone picks one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardFingerprint<'a> {
    /// The probed chip, normalized (`lpa_link::normalize_chip_name`) to the
    /// catalog's `family` spelling, e.g. `"esp32c6"`.
    pub chip: &'a str,
    /// Flash size in bytes from the flash id; `None` when it was not read.
    pub flash_bytes: Option<u64>,
    /// The port's USB vendor id; `None` when the port reports none.
    pub usb_vendor: Option<u16>,
}

/// The catalog boards a [`BoardFingerprint`] leaves standing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardMatch {
    /// The board, when exactly one candidate remains. `None` means "not
    /// sure" — either several boards fit or none does.
    pub board_id: Option<String>,
    /// Every remaining candidate's board id, in catalog order. Empty when
    /// the chip has no board with a served build.
    pub candidates: Vec<String>,
}

/// Narrow the catalog to the boards `fp` could be.
///
/// Candidates are the boards of the fingerprint's chip that resolve a served
/// firmware build (never the desktop target). A known flash size then keeps
/// the boards whose catalog `flash_mb` equals it, and a known USB vendor
/// keeps the boards with no declared bridge or whose bridge has that vendor.
/// A narrowing that would leave nothing is skipped.
pub fn match_boards(fp: &BoardFingerprint) -> BoardMatch {
    let mut candidates: Vec<&BoardDisplayFile> = all_boards()
        .iter()
        .filter(|board| board.family != DESKTOP_FAMILY && board.family == fp.chip)
        .filter(|board| provisioning_build_id(Some(board), Some(fp.chip)).is_some())
        .collect();

    if let Some(flash_bytes) = fp.flash_bytes {
        narrow(&mut candidates, |board| {
            board.flash_mb.map(|mb| u64::from(mb) * MIB) == Some(flash_bytes)
        });
    }
    if let Some(vendor) = fp.usb_vendor {
        narrow(&mut candidates, |board| {
            match board
                .usb_bridge
                .and_then(|bridge| bridge_vendor(bridge_name(bridge)))
            {
                Some(board_vendor) => board_vendor == vendor,
                // No declared bridge, or one whose vendor we cannot name:
                // nothing to contradict the port.
                None => true,
            }
        });
    }

    let candidates: Vec<String> = candidates
        .into_iter()
        .map(|board| board.board_id.clone())
        .collect();
    BoardMatch {
        board_id: match candidates.as_slice() {
            [only] => Some(only.clone()),
            _ => None,
        },
        candidates,
    }
}

/// For each chip with a served build, the build a chip-only install flashes:
/// `provisioning_build_id(None, Some(chip))`, the same join the picker uses,
/// so "the chip's build" means one thing. Served builds only, one entry per
/// chip, in build-def order.
pub fn builds_by_chip() -> Vec<(String, String)> {
    let served = served_build_ids();
    let mut chips: Vec<&str> = Vec::new();
    for build in all_builds() {
        let chip = build.chip.name.as_str();
        if served.iter().any(|id| *id == build.id) && !chips.contains(&chip) {
            chips.push(chip);
        }
    }
    chips
        .into_iter()
        .filter_map(|chip| {
            provisioning_build_id(None, Some(chip)).map(|id| (chip.to_string(), id.to_string()))
        })
        .collect()
}

/// The USB vendor id of a bridge, by the name a board's `usb_bridge` field
/// carries (`native-usb-jtag`, `ch340k`, `cp2102`, ...). `None` for a name
/// this does not know, which constrains nothing.
pub fn bridge_vendor(usb_bridge: &str) -> Option<u16> {
    let name = usb_bridge.to_ascii_lowercase();
    if name == "native-usb-jtag" {
        Some(0x303a)
    } else if name.starts_with("ch340") || name.starts_with("ch343") || name.starts_with("ch9102") {
        Some(0x1a86)
    } else if name.starts_with("cp210") {
        Some(0x10c4)
    } else if name == "ftdi" || name.starts_with("ft") {
        Some(0x0403)
    } else {
        None
    }
}

/// Keep the candidates `keep` accepts, unless that would leave none.
fn narrow(candidates: &mut Vec<&BoardDisplayFile>, keep: impl Fn(&BoardDisplayFile) -> bool) {
    if candidates.iter().any(|board| keep(board)) {
        candidates.retain(|board| keep(board));
    }
}

/// A bridge's name as the sidecar JSON spells it.
fn bridge_name(bridge: UsbBridge) -> &'static str {
    match bridge {
        UsbBridge::NativeUsbJtag => "native-usb-jtag",
        UsbBridge::Ch340G => "ch340g",
        UsbBridge::Ch340C => "ch340c",
        UsbBridge::Ch340K => "ch340k",
        UsbBridge::Ch9102F => "ch9102f",
        UsbBridge::Cp2102 => "cp2102",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XIAO_C6: &str = "seeed/xiao-esp32-c6";
    const DEVKIT_C6: &str = "espressif/esp32-c6-devkitc-1";
    const XIAO_S3_PLUS: &str = "seeed/xiao-esp32-s3-plus";
    const DEVKIT_S3: &str = "espressif/esp32-s3-devkitc-1";

    fn fingerprint(chip: &str, flash_mb: Option<u64>, usb_vendor: Option<u16>) -> BoardMatch {
        match_boards(&BoardFingerprint {
            chip,
            flash_bytes: flash_mb.map(|mb| mb * MIB),
            usb_vendor,
        })
    }

    fn sorted(mut ids: Vec<String>) -> Vec<String> {
        ids.sort();
        ids
    }

    #[test]
    fn c6_with_4_mb_on_native_usb_is_the_xiao() {
        let m = fingerprint("esp32c6", Some(4), Some(0x303a));
        assert_eq!(m.board_id.as_deref(), Some(XIAO_C6));
        assert_eq!(m.candidates, vec![XIAO_C6]);
    }

    #[test]
    fn c6_with_8_mb_is_the_devkitc() {
        let m = fingerprint("esp32c6", Some(8), None);
        assert_eq!(m.board_id.as_deref(), Some(DEVKIT_C6));
    }

    #[test]
    fn c6_of_unknown_size_is_either_board() {
        let m = fingerprint("esp32c6", None, None);
        assert_eq!(m.board_id, None);
        assert_eq!(sorted(m.candidates), vec![DEVKIT_C6, XIAO_C6]);
    }

    #[test]
    fn s3_sizes_pick_their_boards() {
        assert_eq!(
            fingerprint("esp32s3", Some(16), None).board_id.as_deref(),
            Some(XIAO_S3_PLUS)
        );
        assert_eq!(
            fingerprint("esp32s3", Some(8), None).board_id.as_deref(),
            Some(DEVKIT_S3)
        );
    }

    #[test]
    fn classic_behind_a_wch_bridge_keeps_ch340_and_unconstrained_boards() {
        let m = fingerprint("esp32", None, Some(0x1a86));
        assert_eq!(m.board_id, None);
        assert_eq!(
            sorted(m.candidates),
            vec![
                "domraem/dom-z-102",
                "espressif/esp32-devkitc-v4",
                "quinled/dig-uno",
                "quinled/dig2go",
            ]
        );
    }

    #[test]
    fn classic_behind_a_silabs_bridge_drops_the_ch340_boards() {
        // No board declares a CP210x, so the unconstrained ones are all that
        // is left standing.
        let m = fingerprint("esp32", None, Some(0x10c4));
        assert_eq!(
            sorted(m.candidates),
            vec!["espressif/esp32-devkitc-v4", "quinled/dig-uno"]
        );
    }

    #[test]
    fn a_c6_behind_a_wch_bridge_falls_back_to_the_c6_boards() {
        // The C6 DevKitC-1's UART port is a CH343 though the catalog says
        // native USB: the vendor step would empty the set, so it is skipped.
        let m = fingerprint("esp32c6", Some(8), Some(0x1a86));
        assert_eq!(m.board_id.as_deref(), Some(DEVKIT_C6));
        let m = fingerprint("esp32c6", None, Some(0x1a86));
        assert_eq!(sorted(m.candidates), vec![DEVKIT_C6, XIAO_C6]);
    }

    #[test]
    fn a_flash_size_no_board_has_is_skipped() {
        let m = fingerprint("esp32c6", Some(2), None);
        assert_eq!(sorted(m.candidates), vec![DEVKIT_C6, XIAO_C6]);
    }

    #[test]
    fn an_unknown_chip_has_no_candidates() {
        let m = fingerprint("esp8266", Some(4), Some(0x1a86));
        assert_eq!(m.board_id, None);
        assert!(m.candidates.is_empty());
    }

    #[test]
    fn the_desktop_target_is_never_a_candidate() {
        let m = fingerprint(DESKTOP_FAMILY, None, None);
        assert!(m.candidates.is_empty());
    }

    #[test]
    fn every_candidate_resolves_a_served_build() {
        for chip in ["esp32", "esp32c6", "esp32s3"] {
            for id in fingerprint(chip, None, None).candidates {
                let board = all_boards().iter().find(|b| b.board_id == id).unwrap();
                assert!(provisioning_build_id(Some(board), Some(chip)).is_some());
            }
        }
    }

    #[test]
    fn builds_by_chip_lists_the_served_builds() {
        assert_eq!(
            builds_by_chip(),
            vec![
                ("esp32c6".to_string(), "esp32c6-4mb".to_string()),
                ("esp32s3".to_string(), "esp32s3-8mb".to_string()),
                ("esp32".to_string(), "esp32v3-4mb".to_string()),
            ]
        );
    }

    #[test]
    fn bridge_vendors_by_name() {
        assert_eq!(bridge_vendor("native-usb-jtag"), Some(0x303a));
        for name in ["ch340g", "ch340c", "ch340k", "ch343p", "ch9102f"] {
            assert_eq!(bridge_vendor(name), Some(0x1a86), "{name}");
        }
        assert_eq!(bridge_vendor("cp2102"), Some(0x10c4));
        assert_eq!(bridge_vendor("ft232h"), Some(0x0403));
        assert_eq!(bridge_vendor("ftdi"), Some(0x0403));
        assert_eq!(bridge_vendor("pl2303"), None);
    }

    #[test]
    fn bridge_vendor_agrees_with_the_bridge_table() {
        for bridge in [
            UsbBridge::NativeUsbJtag,
            UsbBridge::Ch340G,
            UsbBridge::Ch340C,
            UsbBridge::Ch340K,
            UsbBridge::Ch9102F,
            UsbBridge::Cp2102,
        ] {
            assert_eq!(
                bridge_vendor(bridge_name(bridge)),
                Some(bridge.vid_pid().0),
                "{bridge:?}"
            );
        }
    }
}
