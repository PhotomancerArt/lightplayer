//! What the board says about its relay, for status.

use core::fmt;

use crate::refuse_reason::RefuseReason;

/// The relay client's state, as the board reports it. The wire's own
/// `RelayState` (Wi-Fi roadmap M7, PR B) is mapped from this one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayState {
    /// Cloud relay is off: the board never dials.
    Off,
    /// The board holds no account key: there is no one to register for.
    /// Plugging it into Studio once, signed in, installs one.
    NoAccount,
    /// Not on a network yet, or the last dial could not reach the relay
    /// (the name did not resolve, the connection failed): "Connected, no
    /// internet" when the station is joined.
    WaitingForInternet,
    /// Dialling, registering, or about to dial again after a drop.
    Connecting,
    /// Registered: browsers signed in to the board's accounts can reach it.
    Connected,
    /// The hub refused the board, and why.
    Refused { reason: RefuseReason },
}

impl fmt::Display for RelayState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Off => f.write_str("off"),
            Self::NoAccount => f.write_str("no account"),
            Self::WaitingForInternet => f.write_str("waiting for internet"),
            Self::Connecting => f.write_str("connecting"),
            Self::Connected => f.write_str("connected"),
            Self::Refused { reason } => write!(f, "refused: {reason}"),
        }
    }
}
