//! Which boards have a status LED, and on which pin.
//!
//! Like [`board_quirks_for`](crate::board_quirks_for), the table is keyed on
//! the board id of the manifest **in effect** and lives in code, not in
//! `hardware.json`: a new manifest field would be a persisted-format change,
//! and the same pin is an ordinary GPIO on every other board. A board with no
//! entry has no status LED, and nothing is driven.
//!
//! The pin must also be `reserved_reason`'d in the board's manifest, so no
//! project can claim it; a test below holds the two together.

use crate::{HwGateLevel, XIAO_ESP32_C6_BOARD_ID};

/// A status LED: the GPIO it hangs off and the level that lights it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardStatusLed {
    pub gpio: u8,
    pub lit_level: HwGateLevel,
}

/// The status LED of the board `board_id`, if it has one.
///
/// The match is exact: a board id is an identifier, not a pattern.
pub fn board_status_led_for(board_id: &str) -> Option<BoardStatusLed> {
    match board_id {
        XIAO_ESP32_C6_BOARD_ID => Some(XIAO_C6_USER_LED),
        _ => None,
    }
}

/// The XIAO ESP32-C6's amber user LED: GPIO15, lit when driven LOW (Seeed's
/// pin map names GPIO15 "User Light"; Zephyr's board file declares it
/// `GPIO_ACTIVE_LOW`, RIOT's `LED0_ACTIVE (0)`). GPIO15 is a C6 strapping pin
/// (JTAG source select, read only with an eFuse this product never burns);
/// the firmware first drives it long after the ROM has sampled it.
const XIAO_C6_USER_LED: BoardStatusLed = BoardStatusLed {
    gpio: 15,
    lit_level: HwGateLevel::Low,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HwAddress, default_esp32c6_hardware_manifest};

    #[test]
    fn the_xiao_c6_status_led_is_the_amber_user_led_on_gpio15_active_low() {
        assert_eq!(
            board_status_led_for("seeed/xiao-esp32-c6"),
            Some(BoardStatusLed {
                gpio: 15,
                lit_level: HwGateLevel::Low,
            })
        );
    }

    #[test]
    fn the_status_led_pin_is_reserved_in_the_board_manifest() {
        // The C6 fallback manifest is the XIAO's board file. If the pin were
        // claimable, a project could open it as a button or an output and
        // fight the firmware for it.
        let manifest = default_esp32c6_hardware_manifest();
        let led = board_status_led_for(manifest.board_id()).expect("the XIAO has a status LED");
        let address = HwAddress::new(alloc::format!("/gpio/{}", led.gpio)).unwrap();
        let resource = manifest
            .resources()
            .iter()
            .find(|resource| resource.address() == &address)
            .expect("the status LED pin is in the manifest");

        assert!(
            resource.reserved_reason().is_some(),
            "{address} drives the status LED and must be reserved"
        );
    }

    #[test]
    fn other_boards_have_no_status_led() {
        for board_id in [
            "",
            "SEEED/XIAO-ESP32-C6",
            "seeed/xiao-esp32-c6-plus",
            "espressif/esp32-c6-devkitc-1",
            "espressif/esp32-s3-devkitc-1",
            "quinled/dig2go",
            "domraem/dom-z-102",
        ] {
            assert_eq!(board_status_led_for(board_id), None, "{board_id:?}");
        }
    }
}
