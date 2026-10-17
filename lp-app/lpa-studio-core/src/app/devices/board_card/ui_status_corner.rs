//! [`UiStatusCorner`]: the status corner, cut out of the picture's corner:
//! a quiet ⋯, always, so the card's details are one press away. Its own
//! details hold every notice, how the board is running, the picture's words,
//! and the board's terminal. What needs you is the surface's to say, not the
//! corner's (`docs/ux/anatomy/board-card.md`, "Corner").

use crate::RichSection;

use super::ui_card_action::UiCardAction;
use super::ui_detail_panel::UiDetailPanel;

/// The status corner.
#[derive(Clone, Debug, PartialEq)]
pub struct UiStatusCorner {
    pub mark: CornerMark,
    pub details: UiCornerDetails,
}

/// What the corner knows of the board, for hooks (`data-board-corner`):
/// it draws the same quiet ⋯ for each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CornerMark {
    /// A board Studio is watching.
    Fine,
    /// A board Studio is not watching (offline, its port closed).
    Quiet,
    /// A new board, still saying who it is.
    Blank,
}

/// The corner's details: the same sections and panels the bars' details
/// use, so one renderer draws both.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiCornerDetails {
    /// Every notice on the card (one tinted section each), then "Running".
    pub sections: Vec<RichSection<UiCardAction>>,
    /// The terminal, while the board is linked.
    pub panels: Vec<UiDetailPanel>,
}
