//! What the Wi‑Fi popover says about a board's network, decided once here
//! so the popover, its stories and the app agent's readout say the same
//! thing.

use lpc_wire::server::{NetworkStatus, StationState};

/// The status line, from what the station is doing.
///
/// | station | saved network | sentence |
/// |---|---|---|
/// | `unsupported` | yes | "Saved. This firmware doesn't join Wi‑Fi yet." |
/// | `unsupported` / `off` | no | "Not set." |
/// | `off` | yes | "Off." |
/// | `joining` | yes | "Joining <ssid>…" |
/// | `joined` | yes | "Joined <ssid> · <ip> · <rssi> dBm" |
/// | `failed` | yes | "Couldn't join <ssid>: <reason>" |
pub fn wifi_status_sentence(status: &NetworkStatus) -> String {
    let Some(wifi) = &status.wifi else {
        return NOT_SET.to_string();
    };
    let ssid = &wifi.ssid;
    match &status.station {
        StationState::Unsupported => SAVED_UNSUPPORTED.to_string(),
        StationState::Off => "Off.".to_string(),
        StationState::Joining => format!("Joining {ssid}…"),
        StationState::Joined { ip, rssi } => format!("Joined {ssid} · {ip} · {rssi} dBm"),
        StationState::Failed { reason } => format!("Couldn't join {ssid}: {reason}"),
    }
}

/// The relay line: [`RELAY_ON`] or [`RELAY_OFF`], with "applies once this
/// firmware uses the relay" while the firmware does not join at all.
pub fn cloud_relay_sentence(status: &NetworkStatus) -> String {
    let line = if status.cloud_relay {
        RELAY_ON
    } else {
        RELAY_OFF
    };
    match status.station {
        StationState::Unsupported => format!("{line} · applies once this firmware uses the relay"),
        _ => line.to_string(),
    }
}

/// The cloud relay is on (the default).
pub const RELAY_ON: &str = "Relay on (default)";

/// The cloud relay is switched off.
pub const RELAY_OFF: &str = "Relay off — local network only";

/// No network saved.
pub const NOT_SET: &str = "Not set.";

/// A network saved on a firmware that cannot join yet (every M5 image).
pub const SAVED_UNSUPPORTED: &str = "Saved. This firmware doesn't join Wi‑Fi yet.";

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_wire::server::WifiInfo;

    fn status(station: StationState, saved: bool, cloud_relay: bool) -> NetworkStatus {
        NetworkStatus {
            wifi: saved.then(|| WifiInfo {
                ssid: "lp-walk-net".to_string(),
                has_password: true,
                enabled: true,
            }),
            cloud_relay,
            station,
        }
    }

    #[test]
    fn every_station_state_has_its_sentence() {
        let cases = [
            (
                StationState::Unsupported,
                true,
                SAVED_UNSUPPORTED.to_string(),
            ),
            (StationState::Unsupported, false, NOT_SET.to_string()),
            (StationState::Off, true, "Off.".to_string()),
            (StationState::Off, false, NOT_SET.to_string()),
            (
                StationState::Joining,
                true,
                "Joining lp-walk-net…".to_string(),
            ),
            (
                StationState::Joined {
                    ip: "192.168.1.40".to_string(),
                    rssi: -58,
                },
                true,
                "Joined lp-walk-net · 192.168.1.40 · -58 dBm".to_string(),
            ),
            (
                StationState::Failed {
                    reason: "wrong password".to_string(),
                },
                true,
                "Couldn't join lp-walk-net: wrong password".to_string(),
            ),
        ];
        for (station, saved, sentence) in cases {
            assert_eq!(
                wifi_status_sentence(&status(station.clone(), saved, true)),
                sentence,
                "{station:?}"
            );
        }
    }

    #[test]
    fn the_relay_line_says_whether_it_applies_yet() {
        assert_eq!(
            cloud_relay_sentence(&status(StationState::Unsupported, true, true)),
            "Relay on (default) · applies once this firmware uses the relay"
        );
        assert_eq!(
            cloud_relay_sentence(&status(StationState::Off, true, false)),
            RELAY_OFF
        );
    }
}
