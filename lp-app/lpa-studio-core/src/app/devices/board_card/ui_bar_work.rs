//! [`UiBarWork`]: work in progress, shown in the bar doing it (D34) — the
//! step and the percent while it runs, green for a few seconds when it ends
//! well, striped with Retry when it fails. The picture and the status corner
//! do not change while it runs.

use crate::OfferPath;

use super::ui_card_action::UiCardAction;

/// The work one bar is doing, or has just done.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiBarWork {
    /// The step and the percent ("Sending the project · 40%",
    /// "Updating · 1 of 2 · 40%"), or how it ended.
    pub words: String,
    /// How far along, when it says; drawn as the iridescent fill along the
    /// bar's foot (a sweep when `None`).
    pub percent: Option<u8>,
    pub state: BarWorkState,
    /// The `cancel` offer, while the running work can be cancelled.
    pub cancel: Option<OfferPath>,
    /// Another device's update: the quieter fill.
    pub other_device: bool,
}

/// Where the work stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BarWorkState {
    /// Under way: the spinner, the words, the fill.
    Running,
    /// Ended well, a moment ago: the bar is green for
    /// [`crate::app::devices::activity_ends::DONE_SHOWS_SECS`].
    Done,
    /// Ended badly: the bar is striped until the next try, with Retry when
    /// the bar's own verb is offered again.
    Failed { retry: Option<UiCardAction> },
}
