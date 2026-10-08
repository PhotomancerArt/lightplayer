//! What a device card says about a board it reaches on the LAN: the
//! transport ("Wi‑Fi", the word every surface uses for the link,
//! [`UiLinkKind::label`]) and the address (Wi-Fi M6 P07).
//!
//! Functional, not designed (the card's look for network boards is the
//! device-UX rework's): one line the card draws beside its identity, so a
//! Wi‑Fi board is never mistaken for the USB one beside it, and the address
//! says which socket Studio dialled.

use lpa_devices::Device;
use lpa_link::providers::network_link::{lan_host, url_from_lan_endpoint};

use super::ui_link_kind::UiLinkKind;

/// One Wi-Fi board's card line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiLanLink {
    /// "Wi‑Fi" — the card's word for the link ([`UiLinkKind::label`]).
    pub transport: &'static str,
    /// The host (and port, when given) Studio dialled: `192.168.1.40`,
    /// `lp-b48c.local`.
    pub address: String,
    /// The full socket URL, for a tooltip or a copy.
    pub url: String,
}

impl UiLanLink {
    /// The line as the card draws it: `Wi‑Fi · 192.168.1.40`.
    pub fn line(&self) -> String {
        format!("{} · {}", self.transport, self.address)
    }
}

/// The card line for a device reached on the LAN right now, or `None` for
/// every other device (and a remembered one, which is reached at nothing).
pub fn lan_link_view(device: &Device) -> Option<UiLanLink> {
    let endpoint = device.identity.endpoint.as_ref()?;
    lan_link_for_endpoint(&endpoint.0)
}

/// [`lan_link_view`] for an endpoint key.
pub fn lan_link_for_endpoint(endpoint: &str) -> Option<UiLanLink> {
    let url = url_from_lan_endpoint(endpoint)?;
    Some(UiLanLink {
        transport: UiLinkKind::Wifi.label(),
        address: lan_host(url).to_string(),
        url: url.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lan_board_says_wifi_and_its_address() {
        let line = lan_link_for_endpoint("lan:ws://192.168.1.40/link").expect("a lan endpoint");
        assert_eq!(line.transport, "Wi\u{2011}Fi");
        assert_eq!(line.address, "192.168.1.40");
        assert_eq!(line.url, "ws://192.168.1.40/link");
        assert_eq!(line.line(), "Wi\u{2011}Fi · 192.168.1.40");
    }

    #[test]
    fn the_port_is_part_of_the_address() {
        let line = lan_link_for_endpoint("lan:ws://127.0.0.1:28111/link").unwrap();
        assert_eq!(line.address, "127.0.0.1:28111");
    }

    #[test]
    fn every_other_board_has_no_wifi_line() {
        for endpoint in ["ble:QkxF", "sim:dev1", "emu:dev2", "/dev/cu.usbmodem1"] {
            assert_eq!(lan_link_for_endpoint(endpoint), None, "{endpoint}");
        }
    }
}
