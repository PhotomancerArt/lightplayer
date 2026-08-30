//! The USB vendors an ESP32-class board can enumerate as.
//!
//! One list, two consumers that must agree:
//!
//! - the Web Serial **chooser filter**
//!   (`browser_esp32_device_controller.js`, `ESP32_USB_VENDOR_IDS` — keep
//!   the JS copy in sync by hand; JS is untestable from here, see
//!   `docs/debt/web-serial-js-untestable.md`), and
//! - the **granted-ports sweep** (`sweep_granted_ports` →
//!   `discover_granted`), which under Brave's `SerialAllowAllPortsForUrls`
//!   policy also surfaces Bluetooth serial nodes. Those carry no USB ids at
//!   all, and an unfiltered sweep would mint a junk pending card per node.
//!
//! Deliberately permissive within USB-serial land: any of the four bridge
//! families an ESP32 dev board ships with passes. What it excludes is
//! everything that is not a USB serial bridge — which under the allow-all
//! policy is exactly the junk.

/// Vendor ids the chooser filter offers and the granted-ports sweep accepts.
pub const ESP32_SERIAL_USB_VENDOR_IDS: [u16; 4] = [
    0x303a, // Espressif native USB (C6, S3, …)
    0x1a86, // WCH CH34x bridge (classic ESP32 dev boards)
    0x10c4, // Silicon Labs CP210x bridge
    0x0403, // FTDI bridge
];

/// Whether a granted port's `getInfo()` identity could be an ESP32-class
/// serial bridge. `None` — no USB ids at all — is how a Bluetooth serial
/// node (or any non-USB port) presents, and is refused: a port that cannot
/// name a vendor cannot be one of the bridges we speak to.
pub fn is_esp32_serial_candidate(usb_vid_pid: Option<(u16, u16)>) -> bool {
    usb_vid_pid.is_some_and(|(vendor, _)| ESP32_SERIAL_USB_VENDOR_IDS.contains(&vendor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_bridge_families_pass_and_junk_does_not() {
        for vendor in ESP32_SERIAL_USB_VENDOR_IDS {
            assert!(is_esp32_serial_candidate(Some((vendor, 0x1001))));
        }
        // A Bluetooth serial node under the allow-all policy: no USB ids.
        assert!(!is_esp32_serial_candidate(None));
        // An Arduino Uno is a real serial port and still not ours.
        assert!(!is_esp32_serial_candidate(Some((0x2341, 0x0043))));
    }
}
