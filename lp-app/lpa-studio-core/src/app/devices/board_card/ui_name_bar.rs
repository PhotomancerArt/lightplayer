//! [`UiNameBar`]: the board's name and its one primary action — Connect,
//! Done, Add a project, Unlock, Install firmware or Power on — a flush
//! section of the bar with an icon leading the word; the spinner alone
//! while the board is being reached or identified; or nothing at all when
//! nothing can be pressed (`docs/ux/anatomy/board-card.md`, "Name bar and
//! primary"). The row that is busy, away or waiting says why, once (P2).

use super::ui_card_action::UiCardAction;

/// The name bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiNameBar {
    /// The board's name.
    pub title: String,
    /// Its group or owner, under the name; `None` until the account keeps
    /// boards (Q16).
    pub place: Option<String>,
    /// The one primary; `None` when nothing can be pressed (the board is
    /// busy, away with no way in, busy in another tab, has nothing to send,
    /// or has not said yet): no button, never a greyed verb (P7).
    pub primary: Option<UiPrimary>,
}

/// The name bar's primary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiPrimary {
    /// An offer to press.
    Offer(UiCardAction),
    /// The board is being reached or identified: the spinner alone, no
    /// words — the Link row (or the Hardware row) says how. `label` is the
    /// spinner's accessible name ("Connecting", "Identifying").
    Working { label: String },
}

impl UiPrimary {
    /// The spinner, named `label` for a screen reader.
    pub fn working(label: impl Into<String>) -> Self {
        Self::Working {
            label: label.into(),
        }
    }

    /// The word on the primary: the offer's, or none for the spinner.
    pub fn word(&self) -> Option<&str> {
        match self {
            UiPrimary::Offer(action) => Some(&action.word),
            UiPrimary::Working { .. } => None,
        }
    }
}
