//! What a board says about its cloud relay (Wi-Fi roadmap M7): whether it
//! reached lightplayer.app.

use serde::{Deserialize, Serialize};

/// The relay's state in [`crate::server::NetworkStatus`]. A code, not
/// words: Studio and lp-cli word it.
///
/// A board without a relay client (the S3, the classic, lp-cli's host
/// board) always says [`Self::Off`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RelayState {
    /// Cloud relay is off (the board's switch), or this board has no relay:
    /// it never dials lightplayer.app.
    Off,
    /// The board holds no account key, so there is no one to register for:
    /// plugging it into a signed-in Studio once installs one.
    NoAccount,
    /// Joined, but lightplayer.app could not be reached (the name did not
    /// resolve, the connection failed); the board tries again on its
    /// backoff. Also before the station has an address.
    WaitingForInternet,
    /// Dialling or registering, or about to dial again after a drop.
    Connecting,
    /// Registered: a browser signed in to one of the board's accounts can
    /// reach it from anywhere.
    Connected,
    /// lightplayer.app refused the board, and why.
    Refused { reason: RelayRefusal },
}

/// Why lightplayer.app refused a board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RelayRefusal {
    /// No account key the board holds is one lightplayer.app knows (the
    /// account's key was reset): plugging it into Studio refreshes it.
    UnknownAccount,
    /// The board's firmware speaks a relay protocol lightplayer.app no
    /// longer takes.
    UpdateFirmware,
    /// lightplayer.app cannot take the board right now (busy, or the
    /// account's board limit); the board tries again by itself.
    Busy,
}

impl RelayState {
    /// The state's wire name (`off`, `noAccount`, `waitingForInternet`,
    /// `connecting`, `connected`, `refused`), without its reason — for
    /// summaries and logs.
    #[must_use]
    pub fn kind(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::NoAccount => "noAccount",
            Self::WaitingForInternet => "waitingForInternet",
            Self::Connecting => "connecting",
            Self::Connected => "connected",
            Self::Refused { .. } => "refused",
        }
    }
}

impl RelayRefusal {
    /// The code's wire name, for summaries and logs.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::UnknownAccount => "unknownAccount",
            Self::UpdateFirmware => "updateFirmware",
            Self::Busy => "busy",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_spelling() {
        let cases = [
            (RelayState::Off, r#""off""#),
            (RelayState::NoAccount, r#""noAccount""#),
            (RelayState::WaitingForInternet, r#""waitingForInternet""#),
            (RelayState::Connecting, r#""connecting""#),
            (RelayState::Connected, r#""connected""#),
            (
                RelayState::Refused {
                    reason: RelayRefusal::UnknownAccount,
                },
                r#"{"refused":{"reason":"unknownAccount"}}"#,
            ),
            (
                RelayState::Refused {
                    reason: RelayRefusal::UpdateFirmware,
                },
                r#"{"refused":{"reason":"updateFirmware"}}"#,
            ),
            (
                RelayState::Refused {
                    reason: RelayRefusal::Busy,
                },
                r#"{"refused":{"reason":"busy"}}"#,
            ),
        ];
        for (state, json) in cases {
            assert_eq!(crate::json::to_string(&state).unwrap(), json);
            assert_eq!(crate::json::from_str::<RelayState>(json).unwrap(), state);
            assert!(json.contains(state.kind()), "{json}");
        }
    }
}
