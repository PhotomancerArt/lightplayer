//! "Another tab of this browser holds this board": a fact about the BOARD,
//! not about any link this tab has.
//!
//! Web Serial gives a page no serial number, so a tab cannot tell which of
//! its granted ports is which board before it opens one, and the OS refuses
//! the second `open()` of a held port. What a tab CAN learn is that another
//! tab announced a board by its MAC (the hold edge in `lpa-studio-core`
//! carries those announcements). That arrives here as
//! [`Event::BoardHeld`](crate::Event::BoardHeld), addressed by MAC, and the
//! roster keeps it on every device whose identity names that MAC — however
//! the device and the announcement are ordered (see `Roster`'s held book).
//!
//! Names nothing secret: no tab id, no browser name, no key.

use serde::{Deserialize, Serialize};

/// Which of the board's ways in the other tab holds.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum HoldVia {
    /// The board's USB port (Web Serial).
    Usb,
    /// The board's one network slot (the LAN or the cloud relay).
    Network,
}

/// What the holding tab is doing with the board, as it last said.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum HoldLevel {
    /// It holds the port and is watching: nothing of the user's is open on
    /// the board there.
    Watching,
    /// Its lens or editor is open on the board.
    Open,
    /// It is working on the board (a flash, an update, a push); the label is
    /// the activity's own ("Updating · 42%").
    Busy(String),
}

/// The fact on a board another tab holds.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HeldElsewhere {
    pub via: HoldVia,
    pub level: HoldLevel,
    /// This tab let go because the other tab asked (the card says "Taken by
    /// another tab" instead of "Open in another tab").
    pub taken_from_here: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fact_round_trips_through_json() {
        let held = HeldElsewhere {
            via: HoldVia::Usb,
            level: HoldLevel::Busy("Updating · 42%".to_string()),
            taken_from_here: true,
        };

        let json = serde_json::to_string(&held).expect("serialize");
        let back: HeldElsewhere = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back, held);
    }
}
