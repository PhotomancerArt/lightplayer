//! What a device card says about a board it reaches over the network: the
//! transport ("Wi‑Fi", or "Wi‑Fi via lightplayer.app" — the words every
//! surface uses for the link, [`UiLinkKind::label`]) and, on the LAN, the
//! address (Wi-Fi M6 P07; the relay since the network transport's PR C).
//!
//! Functional, not designed (the card's look for network boards is the
//! device-UX rework's): one line the card draws beside its identity, so a
//! Wi‑Fi board is never mistaken for the USB one beside it, and the address
//! says which socket Studio dialled. The card also reads the line's
//! [`UiLanLink::kind`] for every word that depends on the link, so a board
//! through the relay never wears Bluetooth's or USB's words.

use lpa_devices::Device;
use lpa_link::providers::network_link::{
    board_from_relay_endpoint, lan_host, url_from_lan_endpoint,
};

use super::ui_link_kind::UiLinkKind;

/// One network board's card line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiLanLink {
    /// How the board is reached: [`UiLinkKind::Wifi`] on the LAN,
    /// [`UiLinkKind::Relay`] through lightplayer.app.
    pub kind: UiLinkKind,
    /// "Wi‑Fi", "Wi‑Fi via lightplayer.app" — the card's word for the link
    /// ([`UiLinkKind::label`]).
    pub transport: &'static str,
    /// The host (and port, when given) Studio dialled: `192.168.1.40`,
    /// `lp-b48c.local`. Empty through the relay: the transport names it.
    pub address: String,
    /// The full socket URL on the LAN, for a tooltip or a copy; through the
    /// relay, the endpoint (`relay:<mac>`).
    pub url: String,
}

impl UiLanLink {
    /// The line as the card draws it: `Wi‑Fi · 192.168.1.40`, or
    /// `Wi‑Fi via lightplayer.app`.
    pub fn line(&self) -> String {
        match self.address.is_empty() {
            true => self.transport.to_string(),
            false => format!("{} · {}", self.transport, self.address),
        }
    }
}

/// The card line for a device reached over the network right now, or
/// `None` for every other device (and a remembered one, which is reached at
/// nothing).
pub fn lan_link_view(device: &Device) -> Option<UiLanLink> {
    let endpoint = device.identity.endpoint.as_ref()?;
    lan_link_for_endpoint(&endpoint.0)
}

/// [`lan_link_view`] for an endpoint key.
pub fn lan_link_for_endpoint(endpoint: &str) -> Option<UiLanLink> {
    if board_from_relay_endpoint(endpoint).is_some() {
        return Some(UiLanLink {
            kind: UiLinkKind::Relay,
            transport: UiLinkKind::Relay.label(),
            address: String::new(),
            url: endpoint.to_string(),
        });
    }
    let url = url_from_lan_endpoint(endpoint)?;
    Some(UiLanLink {
        kind: UiLinkKind::Wifi,
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
        assert_eq!(line.kind, UiLinkKind::Wifi);
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
    fn a_board_through_the_relay_says_so_and_nothing_else() {
        let line = lan_link_for_endpoint("relay:a0f26287b48c").expect("a relay endpoint");
        assert_eq!(line.kind, UiLinkKind::Relay);
        assert_eq!(line.line(), "Wi\u{2011}Fi via lightplayer.app");
        assert_eq!(line.url, "relay:a0f26287b48c");
        assert_eq!(lan_link_for_endpoint("relay:nope"), None, "not a board id");
    }

    #[test]
    fn every_other_board_has_no_wifi_line() {
        for endpoint in ["ble:QkxF", "sim:dev1", "emu:dev2", "/dev/cu.usbmodem1"] {
            assert_eq!(lan_link_for_endpoint(endpoint), None, "{endpoint}");
        }
    }
}
