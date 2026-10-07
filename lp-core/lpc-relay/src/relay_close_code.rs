//! The WebSocket close codes the relay ends a leg with.
//!
//! A browser cannot read the HTTP status of a refused upgrade, so every
//! refusal on the browser leg happens **after** the upgrade, as a close
//! code: the 4000s are the relay's own (RFC 6455 §7.4.2 leaves them to
//! applications), the rest are the standard ones. Studio and lp-cli turn
//! each into plain words.

/// One close code and its short reason text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayCloseCode {
    /// 1000: the session ended normally.
    Normal,
    /// 1001: the server is going away (a deploy); come back shortly.
    GoingAway,
    /// 1003: a text message, which no leg carries.
    UnsupportedData,
    /// 1008: something that is not the protocol (a frame out of turn).
    PolicyViolation,
    /// 1009: a message over the relay's frame limit.
    TooBig,
    /// 1011: this end could not keep up (its outbound queue filled).
    Overloaded,
    /// 4401: the browser leg needs a signed-in session (an account or a
    /// guest).
    SignInRequired,
    /// 4404: no board with that id is online.
    BoardOffline,
    /// 4410: the board went away mid-session (its leg closed, or it
    /// reconnected and dropped its old sessions).
    BoardGone,
    /// 4420: too many tries from this address; wait and try again.
    SlowDown,
    /// 4429: the board holds as many sessions as it can.
    Busy,
}

impl RelayCloseCode {
    /// The number on the wire.
    #[must_use]
    pub const fn code(self) -> u16 {
        match self {
            Self::Normal => 1000,
            Self::GoingAway => 1001,
            Self::UnsupportedData => 1003,
            Self::PolicyViolation => 1008,
            Self::TooBig => 1009,
            Self::Overloaded => 1011,
            Self::SignInRequired => 4401,
            Self::BoardOffline => 4404,
            Self::BoardGone => 4410,
            Self::SlowDown => 4420,
            Self::Busy => 4429,
        }
    }

    /// The close code a number names, if it is one of the relay's.
    #[must_use]
    pub const fn from_code(code: u16) -> Option<Self> {
        Some(match code {
            1000 => Self::Normal,
            1001 => Self::GoingAway,
            1003 => Self::UnsupportedData,
            1008 => Self::PolicyViolation,
            1009 => Self::TooBig,
            1011 => Self::Overloaded,
            4401 => Self::SignInRequired,
            4404 => Self::BoardOffline,
            4410 => Self::BoardGone,
            4420 => Self::SlowDown,
            4429 => Self::Busy,
            _ => return None,
        })
    }

    /// What a person is told, in plain words.
    #[must_use]
    pub const fn words(self) -> &'static str {
        match self {
            Self::Normal => "The session ended.",
            Self::GoingAway => "lightplayer.app is restarting — try again in a few seconds.",
            Self::UnsupportedData | Self::PolicyViolation | Self::TooBig => {
                "The relay closed the connection: this client sent something it does not carry."
            }
            Self::Overloaded => "The relay could not keep up — try again.",
            Self::SignInRequired => "Sign in to lightplayer.app to reach boards through it.",
            Self::BoardOffline => "That board is not connected to lightplayer.app right now.",
            Self::BoardGone => "The board went offline.",
            Self::SlowDown => "Too many tries from this network — wait a minute and try again.",
            Self::Busy => "Busy with another connection — try again.",
        }
    }

    /// The close frame's reason text: a stable token, not prose.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Normal => "closed",
            Self::GoingAway => "going-away",
            Self::UnsupportedData => "binary-only",
            Self::PolicyViolation => "protocol",
            Self::TooBig => "too-big",
            Self::Overloaded => "overloaded",
            Self::SignInRequired => "sign-in-required",
            Self::BoardOffline => "board-offline",
            Self::BoardGone => "board-gone",
            Self::SlowDown => "slow-down",
            Self::Busy => "busy",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// These numbers are what Studio and lp-cli match on: pinned.
    #[test]
    fn the_codes_are_pinned() {
        let pinned = [
            (RelayCloseCode::Normal, 1000),
            (RelayCloseCode::GoingAway, 1001),
            (RelayCloseCode::UnsupportedData, 1003),
            (RelayCloseCode::PolicyViolation, 1008),
            (RelayCloseCode::TooBig, 1009),
            (RelayCloseCode::Overloaded, 1011),
            (RelayCloseCode::SignInRequired, 4401),
            (RelayCloseCode::BoardOffline, 4404),
            (RelayCloseCode::BoardGone, 4410),
            (RelayCloseCode::SlowDown, 4420),
            (RelayCloseCode::Busy, 4429),
        ];
        for (close, code) in pinned {
            assert_eq!(close.code(), code, "{}", close.reason());
            assert_eq!(RelayCloseCode::from_code(code), Some(close));
        }
        assert_eq!(RelayCloseCode::from_code(4000), None);
    }
}
