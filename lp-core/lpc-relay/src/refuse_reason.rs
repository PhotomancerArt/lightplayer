//! Why the hub refused a board's registration.

use core::fmt;

/// The hub's reason in a [`RelayFrame::Refused`](crate::RelayFrame::Refused).
///
/// Each code is one byte on the wire. A new reason is a new
/// [`RELAY_PROTO_VERSION`](crate::RELAY_PROTO_VERSION): an older board
/// would read an unknown code as a malformed frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RefuseReason {
    /// No account key the board offered is one the cloud knows: every salt
    /// is unknown or retired (the account reset its key), or no proof
    /// verified. The board waits until its account entries change; Studio
    /// refreshes them over USB.
    UnknownAccount,
    /// The board speaks a relay protocol older than the hub accepts.
    VersionTooOld,
    /// The board speaks a relay protocol newer than the hub knows.
    VersionTooNew,
    /// The account already has as many boards online as the hub holds.
    TooManyBoards,
    /// The board sent something that is not the frame expected next.
    Malformed,
    /// The hub cannot take the board right now; try again later.
    Busy,
}

impl RefuseReason {
    /// The byte on the wire.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::UnknownAccount => 1,
            Self::VersionTooOld => 2,
            Self::VersionTooNew => 3,
            Self::TooManyBoards => 4,
            Self::Malformed => 5,
            Self::Busy => 6,
        }
    }

    /// The reason a byte names, if any.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            1 => Self::UnknownAccount,
            2 => Self::VersionTooOld,
            3 => Self::VersionTooNew,
            4 => Self::TooManyBoards,
            5 => Self::Malformed,
            6 => Self::Busy,
            _ => return None,
        })
    }

    /// Whether the board should keep dialling on its normal backoff. The
    /// others wait for something to change first: new account entries
    /// ([`Self::UnknownAccount`]) or new firmware (the two version
    /// refusals).
    #[must_use]
    pub const fn retries(self) -> bool {
        matches!(self, Self::TooManyBoards | Self::Malformed | Self::Busy)
    }
}

/// The reason's name, as a log line and lp-cli print it.
impl fmt::Display for RefuseReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnknownAccount => "unknown account",
            Self::VersionTooOld => "relay version too old",
            Self::VersionTooNew => "relay version too new",
            Self::TooManyBoards => "too many boards",
            Self::Malformed => "malformed",
            Self::Busy => "busy",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_round_trips_and_unknown_codes_are_none() {
        for reason in [
            RefuseReason::UnknownAccount,
            RefuseReason::VersionTooOld,
            RefuseReason::VersionTooNew,
            RefuseReason::TooManyBoards,
            RefuseReason::Malformed,
            RefuseReason::Busy,
        ] {
            assert_eq!(RefuseReason::from_code(reason.code()), Some(reason));
        }
        assert_eq!(RefuseReason::from_code(0), None);
        assert_eq!(RefuseReason::from_code(7), None);
    }
}
