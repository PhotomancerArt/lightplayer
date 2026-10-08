//! How a board is reached right now, in the words and glyph the card and
//! the editor header use: USB, Bluetooth, Wi‑Fi, or Wi‑Fi via
//! lightplayer.app.
//!
//! One rule, read off the link's endpoint key, so the card's preview
//! sentence, the header's glyph and the lens's refusals can never name
//! different transports for one board (PR C's walk found a Wi‑Fi board
//! wearing the USB glyph and a Bluetooth sentence).

use lpa_devices::identity::EndpointKey;

/// The transport a board is reached over, as a person names it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiLinkKind {
    /// A USB cable: a serial port, and also a sim or an emulated board
    /// (which stand in for one).
    #[default]
    Usb,
    /// A Bluetooth link (`ble:`).
    Bluetooth,
    /// A secure link over the LAN (`lan:`).
    Wifi,
    /// The same secure link, carried by lightplayer.app's relay (`relay:`).
    /// Drawn with the Wi‑Fi glyph: the board is on Wi‑Fi either way.
    Relay,
}

impl UiLinkKind {
    /// The kind of the link at `endpoint` (`None`: no link, which the
    /// header draws as a board on USB, as it always has).
    pub fn of_endpoint(endpoint: Option<&EndpointKey>) -> Self {
        match endpoint {
            Some(endpoint) if endpoint.is_bluetooth() => Self::Bluetooth,
            Some(endpoint) if endpoint.is_lan() => Self::Wifi,
            Some(endpoint) if endpoint.is_relay() => Self::Relay,
            _ => Self::Usb,
        }
    }

    /// Its name in a sentence: "USB", "Bluetooth", "Wi‑Fi", "Wi‑Fi via
    /// lightplayer.app".
    pub fn label(self) -> &'static str {
        match self {
            Self::Usb => "USB",
            Self::Bluetooth => "Bluetooth",
            Self::Wifi => "Wi\u{2011}Fi",
            Self::Relay => "Wi\u{2011}Fi via lightplayer.app",
        }
    }

    /// Whether the board is on Wi‑Fi, reached on the LAN or through the
    /// relay (the Wi‑Fi glyph).
    pub fn is_wifi(self) -> bool {
        matches!(self, Self::Wifi | Self::Relay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_endpoint_names_its_transport() {
        let kind = |key: &str| UiLinkKind::of_endpoint(Some(&EndpointKey(key.to_string())));
        assert_eq!(kind("ble:QkxFLWlk"), UiLinkKind::Bluetooth);
        assert_eq!(kind("lan:ws://192.168.1.40/link"), UiLinkKind::Wifi);
        assert_eq!(kind("usb-1"), UiLinkKind::Usb);
        assert_eq!(UiLinkKind::of_endpoint(None), UiLinkKind::Usb);
        assert_eq!(UiLinkKind::Wifi.label(), "Wi\u{2011}Fi");
        assert_eq!(kind("relay:a0f26287b48c"), UiLinkKind::Relay);
        assert_eq!(
            UiLinkKind::Relay.label(),
            "Wi\u{2011}Fi via lightplayer.app"
        );
        assert!(UiLinkKind::Relay.is_wifi() && UiLinkKind::Wifi.is_wifi());
        assert!(!UiLinkKind::Usb.is_wifi());
    }
}
