//! Why a route closed.

use core::fmt;

/// The reason in a [`RelayFrame::Close`](crate::RelayFrame::Close), sent by
/// either end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RouteCloseReason {
    /// The session ended: its browser closed, or the board closed it.
    Normal,
    /// The board has no free session: it holds as many as it can.
    Busy,
    /// The other end of the route went away (its socket closed).
    Gone,
}

impl RouteCloseReason {
    /// The byte on the wire.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Busy => 1,
            Self::Gone => 2,
        }
    }

    /// The reason a byte names, if any.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            0 => Self::Normal,
            1 => Self::Busy,
            2 => Self::Gone,
            _ => return None,
        })
    }
}

impl fmt::Display for RouteCloseReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Normal => "closed",
            Self::Busy => "busy",
            Self::Gone => "gone",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_round_trips() {
        for reason in [
            RouteCloseReason::Normal,
            RouteCloseReason::Busy,
            RouteCloseReason::Gone,
        ] {
            assert_eq!(RouteCloseReason::from_code(reason.code()), Some(reason));
        }
        assert_eq!(RouteCloseReason::from_code(3), None);
    }
}
