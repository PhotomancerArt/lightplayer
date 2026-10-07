//! What the Wi‑Fi popover says, decided once here so the popover, its
//! stories and the app agent's readout say the same thing. The board sends
//! codes (`wrongPassword`, `notFound`, …); Studio words them, plainly:
//! "Connected", "Not connected", "Wrong password".

use lpc_wire::server::{NetworkStatus, RelayRefusal, RelayState, StationFailure, StationState};

/// How a status word reads: plain, good (connected) or a warning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WifiTone {
    Plain,
    Good,
    Warn,
}

/// A signal strength in words (four bars to one).
pub fn signal_word(rssi: i8) -> &'static str {
    match signal_bars(rssi) {
        4 => "strong",
        3 => "good",
        2 => "fair",
        _ => "weak",
    }
}

/// A signal strength as bars, 1–4.
pub fn signal_bars(rssi: i8) -> u8 {
    match rssi {
        -55.. => 4,
        -65..=-56 => 3,
        -73..=-66 => 2,
        _ => 1,
    }
}

/// The line under the Networks page's header, when it has one: nothing
/// while connected (the connected network leads the list), else why not.
pub fn networks_page_line(status: &NetworkStatus) -> Option<String> {
    match &status.station {
        StationState::Connected { .. } => None,
        StationState::Unsupported if status.networks.is_empty() => {
            Some(NOT_SET_UP_UNSUPPORTED.to_string())
        }
        StationState::Unsupported => Some(NOT_CONNECTED_UNSUPPORTED.to_string()),
        StationState::Off => Some(WIFI_OFF.to_string()),
        StationState::Connecting { ssid, .. } => Some(format!("Connecting to {ssid}…")),
        StationState::Failed {
            ssid,
            reason: StationFailure::WrongPassword,
        } => Some(format!("Not connected — {ssid} refused its password.")),
        StationState::Failed { .. } | StationState::NotConnected => Some(NOT_CONNECTED.to_string()),
    }
}

/// "Connected · <signal> signal · <ip>": a network the board is on.
pub fn connected_line(rssi: i8, ip: &str) -> String {
    format!("Connected · {} signal · {ip}", signal_word(rssi))
}

/// Not connected.
pub const NOT_CONNECTED: &str = "Not connected.";

/// Saved networks on a firmware that cannot connect (every M5 image).
pub const NOT_CONNECTED_UNSUPPORTED: &str =
    "Not connected — this firmware can't connect to Wi‑Fi yet. It will after an update.";

/// Nothing saved, on a firmware that cannot connect.
pub const NOT_SET_UP_UNSUPPORTED: &str =
    "Not set up. This firmware can save a network but can't connect yet — it will after an update.";

/// The board's Wi‑Fi switch is off.
pub const WIFI_OFF: &str = "Wi‑Fi is off.";

/// The connect page's line when nothing is saved and the board can scan.
pub const PICK_A_NETWORK: &str = "Not connected. Pick the board's network:";

/// The connect page on a firmware that cannot scan.
pub const CANNOT_LIST: &str = "This firmware can't list networks. Type the name.";

/// A saved network's page: the password is write-only.
pub const PASSWORD_SAVED: &str = "Password: saved on the board. It can't be shown.";

/// A saved network's page, for an open network.
pub const OPEN_NETWORK: &str = "Open network — no password.";

/// Under an armed Forget.
pub const FORGET_HELP: &str = "The board forgets it and its password.";

/// The cloud relay switch's line.
pub const CLOUD_RELAY_HELP: &str = "Lets lightplayer.app reach this board through the cloud.";

/// What the board says about the cloud relay (Wi-Fi roadmap M7): the line
/// under the Cloud relay switch, and the note under an in-row test.
pub mod relay {
    use super::{NetworkStatus, RelayRefusal, RelayState, StationState, WifiTone};

    pub const REACHING: &str = "Reaching lightplayer.app…";
    pub const CONNECTED: &str = "Connected to lightplayer.app";
    /// Joined, and lightplayer.app did not answer.
    pub const NO_INTERNET: &str = super::test::NO_INTERNET;
    /// The board holds no account key.
    pub const NO_ACCOUNT: &str =
        "Sign in to Studio and plug this board in once to use lightplayer.app";
    /// lightplayer.app no longer knows the board's account key (it was reset).
    pub const REFRESH_ACCOUNT: &str = "Plug this board into Studio once to refresh its account";
    /// lightplayer.app no longer takes the board's relay protocol.
    pub const UPDATE_FIRMWARE: &str = "Update this board's firmware to use lightplayer.app";

    /// The line under the Cloud relay switch, when there is one: nothing
    /// with the switch off, on a board with no relay, or while the hub is
    /// only busy (the board tries again by itself); "Connected, no
    /// internet" only while the station is joined.
    pub fn line(status: &NetworkStatus) -> Option<(&'static str, WifiTone)> {
        if !status.cloud_relay {
            return None;
        }
        let joined = matches!(status.station, StationState::Connected { .. });
        Some(match status.relay {
            RelayState::Off
            | RelayState::Refused {
                reason: RelayRefusal::Busy,
            } => return None,
            RelayState::WaitingForInternet if !joined => return None,
            RelayState::WaitingForInternet => (NO_INTERNET, WifiTone::Warn),
            RelayState::Connecting => (REACHING, WifiTone::Plain),
            RelayState::Connected => (CONNECTED, WifiTone::Good),
            relay => (note(relay)?, WifiTone::Warn),
        })
    }

    /// What a test row says under itself when the relay step is skipped
    /// for a reason the person can act on.
    pub fn note(relay: RelayState) -> Option<&'static str> {
        match relay {
            RelayState::NoAccount => Some(NO_ACCOUNT),
            RelayState::Refused {
                reason: RelayRefusal::UnknownAccount,
            } => Some(REFRESH_ACCOUNT),
            RelayState::Refused {
                reason: RelayRefusal::UpdateFirmware,
            } => Some(UPDATE_FIRMWARE),
            _ => None,
        }
    }
}

/// What the board said about one saved network, as its row reads it.
pub mod row {
    /// Connected · <ip>.
    pub fn connected(ip: &str) -> String {
        format!("Connected · {ip}")
    }
    pub const CONNECTING: &str = "Connecting…";
    pub const WRONG_PASSWORD: &str = "Wrong password";
    pub const IN_RANGE: &str = "In range";
    pub const NOT_IN_RANGE: &str = "Not in range";
    /// The firmware cannot connect, Wi‑Fi is off, or nothing is known yet.
    pub const SAVED: &str = "Saved";
}

/// The popover's own labels: its pages, buttons and fields.
pub mod label {
    pub const WIFI: &str = "Wi‑Fi";
    pub const CONNECT_TO_A_NETWORK: &str = "Connect to a network";
    pub const ADD_BY_NAME: &str = "Add a network by name";
    pub const NEARBY: &str = "Nearby";
    pub const REFRESH: &str = "refresh";
    /// The connect page while the board's radio listens and nothing has
    /// been heard yet (the board answered a scan `scanning`).
    pub const LOOKING_FOR_NETWORKS: &str = "Looking for networks…";
    pub const OTHER_NETWORK: &str = "Other network…";
    pub const OTHER_NETWORK_SUB: &str = "hidden, or not in range";
    pub const OTHER_NETWORK_TITLE: &str = "Other network";
    pub const CHANGE_PASSWORD: &str = "Change password";
    pub const NETWORK_FIELD: &str = "Network";
    pub const PASSWORD_FIELD: &str = "Password";
    pub const DONE: &str = "Done";
    /// The wrong-password test row's forget.
    pub const REMOVE: &str = "Remove";
    pub const WRITING: &str = "Writing to the device…";
    /// A heard network with no password.
    pub const OPEN: &str = "open";
}

/// What a saved network's page says about it.
pub mod page {
    pub const SAVED_UNSUPPORTED: &str = "Saved. This firmware can't connect yet.";
    pub const SAVED_WIFI_OFF: &str = "Saved. Wi‑Fi is off.";
    pub const SAVED: &str = "Saved.";
    pub const WRONG_PASSWORD: &str = "Wrong password — it won't connect until it's changed.";
    pub const NOT_IN_RANGE: &str = "Not in range. It connects when it is.";
    /// In range · <signal> signal.
    pub fn in_range(rssi: i8) -> String {
        format!("In range · {} signal", super::signal_word(rssi))
    }
}

/// The in-row test after Connect (2B): its steps and what each outcome says.
pub mod test {
    /// Looking for <ssid>.
    pub fn looking_for(ssid: &str) -> String {
        format!("Looking for {ssid}")
    }
    pub const CHECKING_PASSWORD: &str = "Checking the password";
    pub const GETTING_ADDRESS: &str = "Getting an address";
    pub const REACHING_CLOUD: &str = "Reaching lightplayer.app";

    pub const CONNECTED: &str = "Connected";
    /// <signal> signal · <ip>.
    pub fn connected_body(rssi: i8, ip: &str) -> String {
        format!("{} signal · {ip}", super::signal_word(rssi))
    }
    pub const WRONG_PASSWORD: &str = "Wrong password";
    pub const WRONG_PASSWORD_BODY: &str =
        "it's saved, but won't connect until the password is changed.";
    pub const NOT_IN_RANGE: &str = "Not in range";
    pub const NOT_IN_RANGE_BODY: &str =
        "it's saved and connects when it's in range. The board only sees 2.4 GHz networks.";
    pub const NO_ADDRESS: &str = "No address";
    pub const NO_ADDRESS_BODY: &str =
        "it's saved, but the network didn't give the board an address.";
    pub const NO_INTERNET: &str = "Connected, no internet";
    /// lightplayer.app didn't answer (<ip>)…
    pub fn no_internet_body(ip: &str) -> String {
        format!("lightplayer.app didn't answer ({ip}). The network may need a sign-in page.")
    }
    pub const SAVED: &str = "Saved";
    pub const SAVED_BODY: &str =
        "this firmware can't connect to Wi‑Fi yet. It will after an update.";
    pub const SAVED_WIFI_OFF_BODY: &str = "Wi‑Fi is off. It connects when Wi‑Fi is on.";
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_wire::server::SavedNetworkInfo;

    fn status(station: StationState, saved: bool) -> NetworkStatus {
        NetworkStatus {
            wifi: true,
            cloud_relay: true,
            networks: if saved {
                vec![SavedNetworkInfo {
                    ssid: "lp-walk-net".to_string(),
                    has_password: true,
                    hidden: false,
                    last: None,
                }]
            } else {
                Vec::new()
            },
            station,
            relay: RelayState::Off,
        }
    }

    #[test]
    fn the_relay_line_says_whether_the_board_reached_lightplayer_app() {
        let joined = StationState::Connected {
            ssid: "lp-walk-net".to_string(),
            ip: "192.168.1.40".to_string(),
            rssi: -48,
            host: "lp-8e30.local".to_string(),
        };
        let cases = [
            (RelayState::Off, None),
            (
                RelayState::Connecting,
                Some(("Reaching lightplayer.app…", WifiTone::Plain)),
            ),
            (
                RelayState::Connected,
                Some(("Connected to lightplayer.app", WifiTone::Good)),
            ),
            (
                RelayState::WaitingForInternet,
                Some(("Connected, no internet", WifiTone::Warn)),
            ),
            (
                RelayState::NoAccount,
                Some((
                    "Sign in to Studio and plug this board in once to use lightplayer.app",
                    WifiTone::Warn,
                )),
            ),
            (
                RelayState::Refused {
                    reason: RelayRefusal::UnknownAccount,
                },
                Some((
                    "Plug this board into Studio once to refresh its account",
                    WifiTone::Warn,
                )),
            ),
            (
                RelayState::Refused {
                    reason: RelayRefusal::UpdateFirmware,
                },
                Some((
                    "Update this board's firmware to use lightplayer.app",
                    WifiTone::Warn,
                )),
            ),
            (
                RelayState::Refused {
                    reason: RelayRefusal::Busy,
                },
                None,
            ),
        ];
        for (relay, line) in cases {
            let mut on = status(joined.clone(), true);
            on.relay = relay;
            assert_eq!(relay::line(&on), line, "{relay:?}");
            on.cloud_relay = false;
            assert_eq!(relay::line(&on), None, "the switch off says nothing");
        }
        let mut away = status(StationState::NotConnected, true);
        away.relay = RelayState::WaitingForInternet;
        assert_eq!(relay::line(&away), None, "no internet only when joined");
    }

    #[test]
    fn the_networks_page_says_why_it_is_not_connected() {
        let cases = [
            (
                StationState::Unsupported,
                true,
                Some(NOT_CONNECTED_UNSUPPORTED),
            ),
            (
                StationState::Unsupported,
                false,
                Some(NOT_SET_UP_UNSUPPORTED),
            ),
            (StationState::Off, true, Some(WIFI_OFF)),
            (StationState::NotConnected, true, Some(NOT_CONNECTED)),
            (
                StationState::Connected {
                    ssid: "lp-walk-net".to_string(),
                    ip: "192.168.1.40".to_string(),
                    rssi: -48,
                    host: "lp-8e30.local".to_string(),
                },
                true,
                None,
            ),
        ];
        for (station, saved, line) in cases {
            assert_eq!(
                networks_page_line(&status(station.clone(), saved)).as_deref(),
                line,
                "{station:?}"
            );
        }
        assert_eq!(
            networks_page_line(&status(
                StationState::Failed {
                    ssid: "lp-walk-net".to_string(),
                    reason: StationFailure::WrongPassword
                },
                true
            ))
            .as_deref(),
            Some("Not connected — lp-walk-net refused its password.")
        );
        assert_eq!(
            networks_page_line(&status(
                StationState::Connecting {
                    ssid: "lp-walk-net".to_string(),
                    step: lpc_wire::ConnectStep::Looking,
                },
                true
            ))
            .as_deref(),
            Some("Connecting to lp-walk-net…")
        );
    }

    #[test]
    fn signal_reads_in_four_words() {
        assert_eq!(signal_word(-41), "strong");
        assert_eq!(signal_word(-55), "strong");
        assert_eq!(signal_word(-60), "good");
        assert_eq!(signal_word(-70), "fair");
        assert_eq!(signal_word(-84), "weak");
        assert_eq!(
            connected_line(-48, "192.168.1.42"),
            "Connected · strong signal · 192.168.1.42"
        );
    }
}
