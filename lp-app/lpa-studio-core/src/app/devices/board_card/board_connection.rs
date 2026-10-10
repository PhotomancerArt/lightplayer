//! [`BoardConnection`]: where this tab's session stands on one board, as
//! its card reads it (the board card ADR, §4, "Connected").
//!
//! The studio controller works it out for every board on the roster
//! (`DeviceRosterView.connections`) from the facts it holds: the connected
//! record and its phase, a Connect's held intent, the lens held across a
//! dropped link, and why the last Connect did not open. A board with none
//! of those is watched, which is every board most of the time.
//!
//! | The controller holds | Connection |
//! |---|---|
//! | nothing about this board | `Watched` |
//! | the connected record, opening; or a Connect held for the board | `Connecting` |
//! | the connected record, open | `Connected` |
//! | the connected record, and the lens held on a dropped link | `Reconnecting` |
//! | the last Connect's failure, on this board | `Failed` |

/// This tab's session on one board.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum BoardConnection {
    /// No session here: the card shows the board's facts, read by the
    /// roster's feed.
    #[default]
    Watched,
    /// A Connect is under way: the board is being reached, or its session
    /// is opening. The card says "Connecting…".
    Connecting,
    /// The session is open on the card: its panel is in the bars' place.
    Connected,
    /// The connected session's link dropped and the lens is holding on for
    /// the board to come back. The card says "Reconnecting…".
    Reconnecting,
    /// The last Connect on this board did not open, and why; the card is
    /// otherwise watched, with Connect as its Retry.
    Failed { reason: String },
}

impl BoardConnection {
    /// The session is open on the card.
    pub fn is_connected(&self) -> bool {
        *self == BoardConnection::Connected
    }
}
