//! The [`LinkInfo`] a board reached through the relay wears (feature
//! `device-link`).
//!
//! Host-compiled, like [`super::lan_link_info`], so the app's routing and
//! card tests build the same facts the browser provider does.

use lpa_devices::identity::EndpointKey;
use lpa_devices::link::LinkInfo;

use super::relay_endpoint::{board_from_relay_socket_url, relay_endpoint, relay_host};

/// The facts for the relay browser leg at `url`: the relay's host as its
/// label (`lightplayer.app`) and the `relay:<board>` endpoint. `None` when
/// `url` is not a relay browser leg. No USB facts and no update channel, as
/// on the LAN.
pub fn relay_link_info(url: &str) -> Option<LinkInfo> {
    let board = board_from_relay_socket_url(url)?;
    Some(LinkInfo {
        label: relay_host(url).to_string(),
        endpoint: EndpointKey(relay_endpoint(board)),
        usb: None,
        serial_number: None,
        carries_update_channel: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relay_link_is_labelled_by_the_relay_and_named_by_its_board() {
        let info = relay_link_info("wss://lightplayer.app/relay/board/a0f26287b48c")
            .expect("a relay browser leg");
        assert_eq!(info.label, "lightplayer.app");
        assert_eq!(info.endpoint.0, "relay:a0f26287b48c");
        assert!(info.endpoint.is_relay() && info.endpoint.is_network());
        assert!(info.usb.is_none() && info.serial_number.is_none());
        assert!(!info.carries_update_channel);
        assert!(relay_link_info("ws://10.0.0.5/link").is_none());
    }
}
