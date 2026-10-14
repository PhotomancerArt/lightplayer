//! [`UiCardAction`]: one button on the board card — a name bar's primary, a
//! bar's action, a verb in a bar's details — as the offer it presses and how
//! the card draws it.
//!
//! An offer's own label is not always the card's word ("Retry" on the
//! `retry` offer, whose label is "Identify again"; "Send latest" on `push`),
//! a press may carry preset values ("Send latest" preselects the project),
//! and some verbs open a picker first. The card says all of that here, so
//! the web never works out which picker or which word, and never needs the
//! `DeviceView` to draw a card (DC25).

use crate::{OfferArgs, OfferPath, UiOffer};

/// One button on the card: the offer it presses, its word, how it is drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiCardAction {
    /// The offer this button presses (`devices/<board>/<verb>`); always one
    /// the view's tree publishes.
    pub offer: OfferPath,
    /// The card's word for it: "Connect" on `reconnect`, "Send latest" on
    /// `push`.
    pub word: String,
    /// The icon leading the word, an icon token as offers use (`usb`,
    /// `lock`, `firmware`, …).
    pub icon: Option<String>,
    /// How the web draws and presses it.
    pub draw: UiActionDraw,
    /// Values the press carries ([`UiActionDraw::Press`]) or the picker
    /// starts with.
    pub args: OfferArgs,
    /// Why the offer is refused here, when it is: drawn disabled with this
    /// reason (the offer's own, never the card's), so "why can't I?" is
    /// answered where it is asked.
    pub refused: Option<String>,
}

/// How the web draws an action and what one press does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiActionDraw {
    /// One press of the offer with the action's args.
    Press,
    /// The project picker (today's `ProjectPickPopover`), which reads only
    /// the board's id to filter the starters.
    ProjectPick { board_id: Option<String> },
    /// The board picker (today's `BoardPickPopover`): the chip it filters
    /// by, and whether the boot banner said it (else the catalog family of
    /// the board's id).
    BoardPick {
        chip: Option<String>,
        chip_from_banner: bool,
    },
    /// The offer's own params, drawn inline (`OfferParamsForm`).
    Choice,
    /// Press the offer unbound: core raises a sheet (Unlock's password).
    Sheet,
}

impl UiCardAction {
    /// One plain press of `offer`, in `word`; refused with the offer's own
    /// reason when it is disabled.
    pub fn press(offer: &UiOffer, word: impl Into<String>) -> Self {
        Self {
            offer: offer.path.clone(),
            word: word.into(),
            icon: None,
            draw: UiActionDraw::Press,
            args: OfferArgs::new(),
            refused: refusal(offer),
        }
    }

    /// The offer in its own words ([`UiOffer::label`]).
    pub fn own_words(offer: &UiOffer) -> Self {
        Self::press(offer, offer.label())
    }

    /// This action with `icon` leading its word.
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// This action drawn as `draw`.
    pub fn drawn(mut self, draw: UiActionDraw) -> Self {
        self.draw = draw;
        self
    }

    /// This action's press carrying `args`.
    pub fn with_args(mut self, args: OfferArgs) -> Self {
        self.args = args;
        self
    }
}

/// The offer's disabled reason, when it is disabled and has no params to
/// fill first (a parameterised offer reads disabled until a value is
/// chosen, which is the picker's job, not a refusal).
fn refusal(offer: &UiOffer) -> Option<String> {
    match &offer.action.meta().enablement {
        crate::ActionEnablement::Enabled => None,
        crate::ActionEnablement::Disabled { .. } if !offer.params().is_empty() => None,
        crate::ActionEnablement::Disabled { reason } => Some(reason.clone()),
    }
}
