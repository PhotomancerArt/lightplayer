//! [`UiStatusCorner`]: the status corner, cut out of the picture's corner.
//! The worst notice's mark (a blue dot when all is fine), then the frame
//! rate or the picture's age; its own details hold every notice, how the
//! board is running, the picture's words, and the board's terminal.

use crate::{RichSection, UiStatusKind};

use super::ui_card_action::UiCardAction;
use super::ui_detail_panel::UiDetailPanel;

/// The status corner.
#[derive(Clone, Debug, PartialEq)]
pub struct UiStatusCorner {
    pub mark: CornerMark,
    /// "58 fps" while the board is live, else the picture's age ("5 h
    /// ago"), else nothing.
    pub reading: Option<String>,
    pub details: UiCornerDetails,
}

/// What the corner shows before its reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CornerMark {
    /// Nothing needs you, on a board Studio is watching: the blue dot.
    Fine,
    /// The worst notice on the card, in its family.
    Notice(UiStatusKind),
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
