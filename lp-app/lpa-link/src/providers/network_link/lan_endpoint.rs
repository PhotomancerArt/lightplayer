//! The endpoint a board on the LAN is reached at: `lan:<url>`.
//!
//! The URL is the board's link socket as Studio dials it
//! (`ws://192.168.1.40/link`, `ws://lp-b48c.local/link`). It is the
//! endpoint's whole identity, the way a Web Bluetooth device id is a `ble:`
//! endpoint's: the model binds a board to it only until the hello's base MAC
//! says which board it is.

/// The endpoint scheme a LAN link wears.
pub const LAN_ENDPOINT_PREFIX: &str = "lan:";

/// The endpoint for the board whose link socket is `url`.
pub fn lan_endpoint(url: &str) -> String {
    format!("{LAN_ENDPOINT_PREFIX}{url}")
}

/// The socket URL a `lan:` endpoint names, or `None` for any other endpoint.
pub fn url_from_lan_endpoint(endpoint: &str) -> Option<&str> {
    endpoint
        .strip_prefix(LAN_ENDPOINT_PREFIX)
        .filter(|url| !url.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lan_endpoint_round_trips_its_url() {
        let endpoint = lan_endpoint("ws://192.168.1.40/link");
        assert_eq!(endpoint, "lan:ws://192.168.1.40/link");
        assert_eq!(
            url_from_lan_endpoint(&endpoint),
            Some("ws://192.168.1.40/link")
        );
    }

    #[test]
    fn other_endpoints_are_not_lan_ones() {
        assert_eq!(url_from_lan_endpoint("ble:QkxF"), None);
        assert_eq!(url_from_lan_endpoint("/dev/cu.usbmodem1"), None);
        assert_eq!(url_from_lan_endpoint("lan:"), None);
    }
}
