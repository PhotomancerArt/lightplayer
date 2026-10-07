//! What a device card says about a board it reaches on the LAN: the
//! transport ("Wi-Fi") and the address (Wi-Fi M6 P07).
//!
//! Functional, not designed (the card's look for network boards is roadmap
//! M8): one line the card draws beside its identity, so a Wi-Fi board is
//! never mistaken for the USB one beside it, and the address says which
//! socket Studio dialled.

use lpa_devices::Device;
use lpa_link::LinkProviderKind;
use lpa_link::providers::network_link::{lan_host, url_from_lan_endpoint};

/// One Wi-Fi board's card line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiLanLink {
    /// "Wi-Fi" — the provider kind's own transport label.
    pub transport: &'static str,
    /// The host (and port, when given) Studio dialled: `192.168.1.40`,
    /// `lp-b48c.local`.
    pub address: String,
    /// The full socket URL, for a tooltip or a copy.
    pub url: String,
}

impl UiLanLink {
    /// The line as the card draws it: `Wi-Fi · 192.168.1.40`.
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
        transport: LinkProviderKind::BrowserWebsocket
            .transport_label()
            .unwrap_or("Wi-Fi"),
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
        assert_eq!(line.transport, "Wi-Fi");
        assert_eq!(line.address, "192.168.1.40");
        assert_eq!(line.url, "ws://192.168.1.40/link");
        assert_eq!(line.line(), "Wi-Fi · 192.168.1.40");
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
