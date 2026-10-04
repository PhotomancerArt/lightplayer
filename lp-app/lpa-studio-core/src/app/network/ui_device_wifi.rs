//! [`UiDeviceWifi`]: what the device card's Wi‑Fi row and its popover read.

use lpa_devices::DeviceId;
use lpc_wire::server::NetworkStatus;

use super::wifi_status_sentence::{lan_only_sentence, wifi_status_sentence};

/// One device's Wi‑Fi facts, joined onto its card (the Connections
/// group's Wi‑Fi row). Present for every LightPlayer board on a link that
/// has unlocked (plan Q6); the controls themselves are offers at
/// `devices/<board>/wifi/…`, published only while the link holds edit.
///
/// It carries no password: Studio never has one to show (plan Q8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiDeviceWifi {
    pub device: DeviceId,
    /// The link holds edit (author): the board is asked, and the offers are
    /// published. `false`: the row says [`NEEDS_AUTHOR`] and nothing is
    /// asked.
    pub can_edit: bool,
    /// The board's last answer; `None` until it has answered one.
    pub status: Option<NetworkStatus>,
    /// A read is in flight.
    pub reading: bool,
    /// A change is in flight.
    pub writing: bool,
    /// The last change's (or read's) failure, in the board's words.
    pub error: Option<String>,
}

impl UiDeviceWifi {
    /// The row's value: the saved network's name, "not set", or nothing
    /// yet.
    pub fn row_value(&self) -> &str {
        match &self.status {
            Some(status) => status
                .wifi
                .as_ref()
                .map_or("not set", |wifi| wifi.ssid.as_str()),
            None if self.can_edit => "",
            None => "needs author",
        }
    }

    /// The status sentence ([`wifi_status_sentence`]), once read.
    pub fn status_line(&self) -> Option<String> {
        self.status.as_ref().map(wifi_status_sentence)
    }

    /// The relay line ([`lan_only_sentence`]), once read.
    pub fn lan_line(&self) -> Option<String> {
        self.status.as_ref().map(lan_only_sentence)
    }

    /// What the popover says above the form while there is no status:
    /// [`READING`] or [`NEEDS_AUTHOR`].
    pub fn waiting_line(&self) -> Option<&'static str> {
        match (&self.status, self.can_edit) {
            (_, false) => Some(NEEDS_AUTHOR),
            (None, true) => Some(READING),
            (Some(_), true) => None,
        }
    }
}

/// The popover's one line about what it is.
pub const WIFI_ABOUT: &str =
    "The network this board joins. Saved on the board; the password can't be read back.";

/// A link below author: nothing is asked, nothing offered.
pub const NEEDS_AUTHOR: &str = "Needs Author access — unlock with an author password.";

/// The status is on its way.
pub const READING: &str = "Reading…";

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_wire::server::{StationState, WifiInfo};

    fn wifi(status: Option<NetworkStatus>, can_edit: bool) -> UiDeviceWifi {
        UiDeviceWifi {
            device: DeviceId(1),
            can_edit,
            status,
            reading: false,
            writing: false,
            error: None,
        }
    }

    #[test]
    fn the_row_names_the_network_or_says_why_not() {
        let saved = NetworkStatus {
            wifi: Some(WifiInfo {
                ssid: "lp-walk-net".to_string(),
                has_password: true,
                enabled: true,
            }),
            lan_only: false,
            station: StationState::Unsupported,
        };
        assert_eq!(wifi(Some(saved.clone()), true).row_value(), "lp-walk-net");
        assert_eq!(
            wifi(
                Some(NetworkStatus {
                    wifi: None,
                    ..saved.clone()
                }),
                true
            )
            .row_value(),
            "not set"
        );
        assert_eq!(wifi(None, true).waiting_line(), Some(READING));
        assert_eq!(wifi(None, false).waiting_line(), Some(NEEDS_AUTHOR));
        assert_eq!(wifi(Some(saved), true).waiting_line(), None);
    }
}
