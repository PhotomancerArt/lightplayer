//! Formatting for the chip identity each per-SOC firmware injects.
//!
//! The identity itself — factory MAC, silicon revision, 802.15.4 EUI-64 —
//! lives in efuse, and reading it needs `esp_hal`, which this crate
//! deliberately does not depend on (ADR 2026-07-29-per-chip-fw-toolchains:
//! chip-generic code builds under both toolchains, and **chip facts
//! arrive by injection**). So each leaf firmware reads its own efuse and
//! hands the result to `LpServer::set_hardware_identity`; what is shared
//! here is only the wire formatting, which every chip must agree on.
//!
//! Why the BASE MAC and not a list of per-interface ones: Wi-Fi Station
//! *is* the base address, and SoftAP/BLE are derived from it by a
//! published rule (set the local-admin bit; BLE additionally bumps the
//! last octet). Reporting the derivations would be redundant and would
//! drift from what the radios actually use — and `esp_hal` exposes
//! `override_mac_address`, so a derived value could be a lie outright. An
//! interface reports its OWN address when that interface is genuinely
//! wired, the same rule the rest of `HardwareFacts` follows.
//!
//! The base MAC matters beyond display: it is the only identity of a
//! board that survives an erase. The `dev…` uid Studio stamps lives in
//! the device filesystem and dies with it.

extern crate alloc;

use alloc::format;
use alloc::string::String;

/// Lowercase colon-separated hex — the form everything else prints a
/// hardware address in, so it can be compared by eye against `ip link`, a
/// router's client list, or an esptool dump without re-formatting.
///
/// Takes a slice rather than `[u8; 6]` so the same formatting serves the
/// 8-byte 802.15.4 EUI-64.
pub fn hex_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            out.push(':');
        }
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mac_renders_as_lowercase_colon_hex() {
        assert_eq!(
            hex_bytes(&[0xAA, 0x0B, 0xcc, 0xdd, 0xee, 0xff]),
            "aa:0b:cc:dd:ee:ff",
            "zero-padded, lowercase, colon-separated"
        );
    }

    #[test]
    fn the_same_formatting_serves_an_eui64() {
        assert_eq!(
            hex_bytes(&[0x60, 0x55, 0xf9, 0xff, 0xfe, 0x01, 0x02, 0x03]),
            "60:55:f9:ff:fe:01:02:03",
            "eight bytes, same shape — the width is the radio's, not the format's"
        );
    }
}
