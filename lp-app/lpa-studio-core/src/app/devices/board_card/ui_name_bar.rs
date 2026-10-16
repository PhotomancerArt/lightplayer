//! [`UiNameBar`]: the board's name and its one primary action — Connect,
//! Done, Add a project, Unlock, Install or Power on — a flush section of
//! the bar with an icon leading the word; or, when nothing can be pressed,
//! why, in the primary's own word.

use super::ui_card_action::UiCardAction;

/// The name bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiNameBar {
    /// The board's name.
    pub title: String,
    /// Its group or owner, under the name; `None` until the account keeps
    /// boards (Q16).
    pub place: Option<String>,
    /// The one primary action; `None` only where a session holds the board
    /// and Done is not offered.
    pub primary: Option<UiPrimary>,
}

/// The name bar's primary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiPrimary {
    /// An offer to press.
    Offer(UiCardAction),
    /// Nothing to press: the primary says why in its own word —
    /// "Connecting…", "Updating · 42%", "Offline · 2 weeks",
    /// "Identifying…". Never a greyed verb with the reason in a tooltip.
    /// `reason` is the whole of why, for the hover; most often the word
    /// itself.
    Unavailable {
        word: String,
        icon: String,
        reason: String,
    },
}

impl UiPrimary {
    /// A primary that says `words` (its word and its reason), with `icon`.
    pub fn says(words: impl Into<String>, icon: impl Into<String>) -> Self {
        let words = words.into();
        Self::Unavailable {
            reason: words.clone(),
            word: words,
            icon: icon.into(),
        }
    }

    /// The word on the primary, either way.
    pub fn word(&self) -> &str {
        match self {
            UiPrimary::Offer(action) => &action.word,
            UiPrimary::Unavailable { word, .. } => word,
        }
    }
}
