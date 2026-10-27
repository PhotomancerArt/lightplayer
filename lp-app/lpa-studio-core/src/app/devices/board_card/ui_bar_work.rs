//! [`UiBarWork`]: work in progress, shown in the row doing it (D34) — a
//! state of the row, not a second line: the spinner takes the icon's place
//! and the work's words are the row's state ("sending · 40%"), or, on a
//! row with no name while it works, its value ("USB connecting"). Green for a few seconds
//! when it ends well, striped with Retry when it fails, the reason in the
//! details. The picture and the status corner do not change while it runs.
//!
//! While the card's top says the same work (its sentence, its bar and its
//! number), the row is carried ([`UiBarWork::carried`]): it says the verb
//! alone ("updating"), its spinner in the icon's place and no fill. One
//! work, one bar, one number (P2).

use crate::OfferPath;

use super::ui_card_action::UiCardAction;

/// The work one row is doing, or has just done.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiBarWork {
    /// The row's state while it works, in label voice: "sending · 40%",
    /// "updating · 42%", "installing · 30%", "identifying", or how it
    /// ended ("sent", "send failed"), a link row's ("connecting",
    /// "connection failed"). Empty while the row only
    /// waits for the board to say ("◌ Project").
    pub words: String,
    /// The whole of why it failed, for the row's details and the status
    /// corner: never drawn on the row (P5).
    pub detail: Option<String>,
    /// How far along, when it says; drawn as the iridescent fill along the
    /// row's foot (a sweep when `None`).
    pub percent: Option<u8>,
    pub state: BarWorkState,
    /// The `cancel` offer, while the running work can be cancelled.
    pub cancel: Option<OfferPath>,
    /// Another device's update: the quieter fill.
    pub other_device: bool,
    /// The card's top says this work, with its bar and its number: the row
    /// says only its verb and draws no fill; its spinner stays (P2).
    pub carried: bool,
}

impl UiBarWork {
    /// Work under way, saying `words`, `percent` along when it knows.
    pub fn running(words: impl Into<String>, percent: Option<u8>) -> Self {
        Self {
            words: words.into(),
            detail: None,
            percent,
            state: BarWorkState::Running,
            cancel: None,
            other_device: false,
            carried: false,
        }
    }

    /// Work that failed, saying `words` on the row and `detail` in its
    /// details, with `retry` when the row's own verb is offered again.
    pub fn failed(
        words: impl Into<String>,
        detail: Option<String>,
        retry: Option<UiCardAction>,
    ) -> Self {
        Self {
            words: words.into(),
            detail,
            percent: None,
            state: BarWorkState::Failed { retry },
            cancel: None,
            other_device: false,
            carried: false,
        }
    }
}

impl UiBarWork {
    /// This work as the row says it while the card's top carries it: the
    /// verb alone ("updating", not "updating · 42%"), no fill. The percent
    /// stays, for the top to read; only running work is carried.
    pub fn carried_by_surface(self) -> Self {
        if self.state != BarWorkState::Running {
            return self;
        }
        let words = match self.percent {
            Some(percent) => {
                let number = format!(" \u{b7} {}", crate::percent(u32::from(percent)));
                self.words
                    .strip_suffix(number.as_str())
                    .map_or_else(|| self.words.clone(), str::to_string)
            }
            None => self.words.clone(),
        };
        Self {
            words,
            carried: true,
            ..self
        }
    }
}

/// Where the work stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BarWorkState {
    /// Under way: the spinner, the words, the fill.
    Running,
    /// Ended well, a moment ago: the row is green for
    /// [`crate::app::devices::activity_ends::DONE_SHOWS_SECS`].
    Done,
    /// Ended badly: the row is striped until the next try, with Retry when
    /// the row's own verb is offered again.
    Failed { retry: Option<UiCardAction> },
}
