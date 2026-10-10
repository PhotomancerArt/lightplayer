//! [`UiNameBar`]: the board's name and its one primary action — Connect,
//! Unlock, Install, Edit (until "connected" lands) or Power on — a flush
//! section of the bar with an icon leading the word.

use super::ui_card_action::UiCardAction;

/// The name bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiNameBar {
    /// The board's name.
    pub title: String,
    /// Its group or owner, under the name; `None` until the account keeps
    /// boards (Q16).
    pub place: Option<String>,
    /// The one primary action; `None` while the editor holds the board
    /// (until Done lands).
    pub primary: Option<UiPrimary>,
}

/// The name bar's primary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiPrimary {
    /// An offer to press.
    Offer(UiCardAction),
    /// The word the primary would be, drawn disabled, saying why
    /// ("Offline · 2 weeks", "Busy: Updating · 42%").
    Unavailable {
        word: String,
        icon: String,
        reason: String,
    },
}

impl UiPrimary {
    /// The word on the primary, either way.
    pub fn word(&self) -> &str {
        match self {
            UiPrimary::Offer(action) => &action.word,
            UiPrimary::Unavailable { word, .. } => word,
        }
    }
}
