//! What a board's Wi-Fi station is doing, as it reports it.

use alloc::string::String;
use serde::{Deserialize, Serialize};

/// The station's state in [`crate::server::NetworkStatus`].
///
/// Every image before the station lands (Wi-Fi roadmap M6) says
/// [`Self::Unsupported`]; the other states are defined now so a client can
/// render them before any firmware produces them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StationState {
    /// This firmware does not join Wi-Fi.
    Unsupported,
    /// No network saved, or one saved and switched off.
    Off,
    /// Trying to join the saved network.
    Joining,
    /// Joined: the board's address and the signal strength.
    Joined {
        /// Dotted IPv4 address.
        ip: String,
        /// Signal strength in dBm.
        rssi: i8,
    },
    /// The last attempt failed, in words a person can act on.
    Failed { reason: String },
}

impl StationState {
    /// The state's wire name (`unsupported`, `off`, `joining`, `joined`,
    /// `failed`), without its fields — for summaries and logs.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Off => "off",
            Self::Joining => "joining",
            Self::Joined { .. } => "joined",
            Self::Failed { .. } => "failed",
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
            (StationState::Joining, r#""joining""#),
            (
                StationState::Joined {
                    ip: "192.168.1.40".to_string(),
                    rssi: -61,
                },
                r#"{"joined":{"ip":"192.168.1.40","rssi":-61}}"#,
            ),
            (
                StationState::Failed {
                    reason: "wrong password".to_string(),
                },
                r#"{"failed":{"reason":"wrong password"}}"#,
            ),
        ];
        for (state, json) in cases {
            assert_eq!(crate::json::to_string(&state).unwrap(), json);
            assert_eq!(crate::json::from_str::<StationState>(json).unwrap(), state);
        }
    }
}
