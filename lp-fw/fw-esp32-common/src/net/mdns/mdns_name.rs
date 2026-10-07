//! The board's name on the LAN.

use alloc::string::String;
use core::fmt::Write as _;

/// The board's mDNS label, `lp-xxxx`: the last two base-MAC bytes in
/// lower-case hex, the same four hex digits as its Bluetooth name `LP-xxxx`
/// (plan Q6, MD13).
#[must_use]
pub fn mdns_label(base_mac: [u8; 6]) -> String {
    let mut label = String::with_capacity(7);
    let _ = write!(label, "lp-{:02x}{:02x}", base_mac[4], base_mac[5]);
    label
}

/// The board's full mDNS name, `lp-xxxx.local`: what
/// [`lpc_wire::StationState::Connected`] reports as `host`.
#[must_use]
pub fn mdns_host(base_mac: [u8; 6]) -> String {
    let mut host = mdns_label(base_mac);
    host.push_str(".local");
    host
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_is_the_last_two_mac_bytes_like_the_bluetooth_name() {
        let mac = [0x10, 0xbd, 0xa3, 0xb0, 0x8e, 0x30];
        assert_eq!(mdns_label(mac), "lp-8e30");
        assert_eq!(mdns_host(mac), "lp-8e30.local");
        assert_eq!(mdns_host([0, 0, 0, 0, 0x0a, 0xb0]), "lp-0ab0.local");
    }
}
