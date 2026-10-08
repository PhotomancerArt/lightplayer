//! The [`LinkInfo`] a LAN board's link wears (feature `device-link`).
//!
//! Host-compiled so the app's routing and card tests build the same facts
//! the browser provider does (`device_link::browser_websocket`).

use lpa_devices::identity::EndpointKey;
use lpa_devices::link::LinkInfo;

use super::lan_endpoint::lan_endpoint;

/// The facts for the board whose link socket is `url`: the host it is
/// reached at as its label, and the `lan:<url>` endpoint. No USB ids and no
/// serial number — identity comes from the hello's base MAC, like every
/// other transport. Its secure lp-link carries the update channel (OTA M8);
/// whether the board speaks it is the board's own announcement (DS9).
pub fn lan_link_info(url: &str) -> LinkInfo {
    LinkInfo {
        label: lan_host(url).to_string(),
        endpoint: EndpointKey(lan_endpoint(url)),
        usb: None,
        serial_number: None,
        carries_update_channel: true,
    }
}

/// The host (and port, when given) a socket URL names:
/// `ws://192.168.1.40:8080/link` → `192.168.1.40:8080`.
pub fn lan_host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split(['/', '?', '#']).next().unwrap_or(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lan_link_is_labelled_by_its_host_and_carries_the_update_channel() {
        let info = lan_link_info("ws://lp-b48c.local/link");
        assert_eq!(info.label, "lp-b48c.local");
        assert_eq!(info.endpoint.0, "lan:ws://lp-b48c.local/link");
        assert!(info.usb.is_none() && info.serial_number.is_none());
        assert!(info.carries_update_channel);
    }

    #[test]
    fn the_host_keeps_its_port_and_drops_the_path() {
        assert_eq!(lan_host("ws://10.0.0.9:8080/link?x=1"), "10.0.0.9:8080");
        assert_eq!(lan_host("10.0.0.9"), "10.0.0.9");
    }
}
