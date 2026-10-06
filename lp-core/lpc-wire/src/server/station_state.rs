//! What a board's Wi-Fi station is doing, as it reports it.

use alloc::string::String;
use serde::{Deserialize, Serialize};

use crate::server::station_failure::StationFailure;

/// The station's state in [`crate::server::NetworkStatus`].
///
/// Every image before the station lands (Wi-Fi roadmap M6) says
/// [`Self::Unsupported`]; the other states are defined now so a client can
/// render them before any firmware produces them. Which saved network the
/// station tries is its own call: the strongest it hears, skipping one
/// whose password was refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StationState {
    /// This firmware does not connect to Wi-Fi.
    Unsupported,
    /// The board's Wi-Fi switch is off.
    Off,
    /// On, and not connected: nothing saved, or none of the saved networks
    /// in range.
    NotConnected,
    /// Trying the saved network `ssid`.
    Connecting { ssid: String },
    /// Connected to `ssid`: the board's address and the signal strength.
    Connected {
        ssid: String,
        /// Dotted IPv4 address.
        ip: String,
        /// Signal strength in dBm.
        rssi: i8,
    },
    /// The attempt at `ssid` failed, and why.
    Failed {
        ssid: String,
        reason: StationFailure,
    },
}

impl StationState {
    /// The state's wire name (`unsupported`, `off`, `notConnected`,
    /// `connecting`, `connected`, `failed`), without its fields — for
    /// summaries and logs.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Off => "off",
            Self::NotConnected => "notConnected",
            Self::Connecting { .. } => "connecting",
            Self::Connected { .. } => "connected",
            Self::Failed { .. } => "failed",
        }
    }

    /// The network the state is about, when it is about one.
    #[must_use]
    pub fn ssid(&self) -> Option<&str> {
        match self {
            Self::Connecting { ssid }
            | Self::Connected { ssid, .. }
            | Self::Failed { ssid, .. } => Some(ssid),
            Self::Unsupported | Self::Off | Self::NotConnected => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn every_state_spelling() {
        let cases = [
            (StationState::Unsupported, r#""unsupported""#),
            (StationState::Off, r#""off""#),
            (StationState::NotConnected, r#""notConnected""#),
            (
                StationState::Connecting {
                    ssid: "lp-walk-net".to_string(),
                },
                r#"{"connecting":{"ssid":"lp-walk-net"}}"#,
            ),
            (
                StationState::Connected {
                    ssid: "lp-walk-net".to_string(),
                    ip: "192.168.1.40".to_string(),
                    rssi: -61,
                },
                r#"{"connected":{"ssid":"lp-walk-net","ip":"192.168.1.40","rssi":-61}}"#,
            ),
            (
                StationState::Failed {
                    ssid: "lp-walk-net".to_string(),
                    reason: StationFailure::WrongPassword,
                },
                r#"{"failed":{"ssid":"lp-walk-net","reason":"wrongPassword"}}"#,
            ),
            (
                StationState::Failed {
                    ssid: "lp-walk-net".to_string(),
                    reason: StationFailure::NotFound,
                },
                r#"{"failed":{"ssid":"lp-walk-net","reason":"notFound"}}"#,
            ),
            (
                StationState::Failed {
                    ssid: "lp-walk-net".to_string(),
                    reason: StationFailure::NoAddress,
                },
                r#"{"failed":{"ssid":"lp-walk-net","reason":"noAddress"}}"#,
            ),
        ];
        for (state, json) in cases {
            assert_eq!(crate::json::to_string(&state).unwrap(), json);
            assert_eq!(crate::json::from_str::<StationState>(json).unwrap(), state);
        }
    }
}
