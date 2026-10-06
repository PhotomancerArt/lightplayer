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
//!
//! A third consumer reads the same vendor id for a different question:
//! [`link_config_for_usb_vendor`], which lp-link preset a Web Serial port's
//! link runs — `uart()` behind a bridge (the classic ESP32's CH340), `usb()`
//! on Espressif's native USB. It is the browser's copy of the native host's
//! rule (`lpa_client::transport_serial::link_config_for_port`).

// lpc-wire is optional (`device-link`); every caller of the preset rule is
// a provider that implies it. `test`: the unit test below reaches it through
// the dev-dependency, so a featureless `cargo test -p lpa-link` builds.
#[cfg(any(feature = "device-link", test))]
use lpc_wire::lp_link::LinkConfig;

/// Vendor ids the chooser filter offers and the granted-ports sweep accepts.
pub const ESP32_SERIAL_USB_VENDOR_IDS: [u16; 4] = [
    0x303a, // Espressif native USB (C6, S3, …)
    0x1a86, // WCH CH34x bridge (classic ESP32 dev boards)
    0x10c4, // Silicon Labs CP210x bridge
    0x0403, // FTDI bridge
];

/// Espressif's native USB-Serial-JTAG (C6, S3): the one vendor whose port is
/// the chip itself rather than a bridge in front of a UART.
pub const ESPRESSIF_NATIVE_USB_VENDOR_ID: u16 = 0x303a;

/// Whether a granted port's `getInfo()` identity could be an ESP32-class
/// serial bridge. `None` — no USB ids at all — is how a Bluetooth serial
/// node (or any non-USB port) presents, and is refused: a port that cannot
/// name a vendor cannot be one of the bridges we speak to.
pub fn is_esp32_serial_candidate(usb_vid_pid: Option<(u16, u16)>) -> bool {
    usb_vid_pid.is_some_and(|(vendor, _)| ESP32_SERIAL_USB_VENDOR_IDS.contains(&vendor))
}

/// The lp-link preset for a serial port, from the USB vendor id the port
/// enumerated with.
///
/// Any vendor but Espressif's native USB is a bridge chip in front of UART0 —
/// on this product, the classic ESP32 (v3) behind a CH340 (`0x1a86`) — so its
/// link is [`LinkConfig::uart()`]. Espressif's own vendor id is the C6/S3's
/// USB-Serial-JTAG: [`LinkConfig::usb()`]. So is a port with no USB ids,
/// which is the pre-bridge-aware behaviour and harmless: the board advertises
/// its own receive window in the handshake, and a `usb()` host sends no more
/// than that (plan `lp2025/2026-09-28-2015-classic-uart-on-lp-link` P3/P4).
///
/// The same rule as the native host's `link_config_for_port`, which keys off
/// the same vendor id; the two must agree, or one board would get a
/// different preset depending on which host opened it.
#[cfg(any(feature = "device-link", test))]
pub fn link_config_for_usb_vendor(usb_vendor_id: Option<u16>) -> LinkConfig {
    match usb_vendor_id {
        Some(vendor) if vendor != ESPRESSIF_NATIVE_USB_VENDOR_ID => LinkConfig::uart(),
        _ => LinkConfig::usb(),
    }
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

    #[test]
    fn a_bridge_port_runs_the_uart_preset_and_native_usb_the_usb_one() {
        // `LinkConfig` has no `PartialEq`; its `Debug` names every field.
        let preset = |config: LinkConfig| format!("{config:?}");
        // The desk classic's CH340K, and the other bridges a dev board ships.
        for vendor in [0x1a86, 0x10c4, 0x0403] {
            assert_eq!(
                preset(link_config_for_usb_vendor(Some(vendor))),
                preset(LinkConfig::uart()),
                "{vendor:#06x}"
            );
        }
        assert_eq!(
            preset(link_config_for_usb_vendor(Some(
                ESPRESSIF_NATIVE_USB_VENDOR_ID
            ))),
            preset(LinkConfig::usb())
        );
        // No USB ids at all: today's preset, unchanged.
        assert_eq!(
            preset(link_config_for_usb_vendor(None)),
            preset(LinkConfig::usb())
        );
        assert_ne!(preset(LinkConfig::uart()), preset(LinkConfig::usb()));
    }
}
