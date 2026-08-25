//! The runtime card's UI VIEW-STATE — core-owned (2026-07-25 re-home).
//!
//! What the user is inspecting on a card (the selected [`CardTab`]) and any
//! card-resident flow that's open (a [`CardSheet`]). This used to live in
//! the web renderer's `use_signal`s, which meant it died when a card's
//! component instance did — amnesia across the card ⇄ pane growth (D43),
//! and unreachable by e2e (tests could only dispatch the underlying op,
//! never drive open→confirm). Core ownership fixes both: the state
//! survives module mode changes and is drivable past the dispatch
//! boundary.
//!
//! The state is keyed by a card's canonical [identity] so it follows the
//! runtime, not the widget: growing a card into the editor pane keeps the
//! same tab and any open sheet.
//!
//! ⚠️ The device half is gone (M2 of the device-model rebuild): `CardOp` /
//! `CardOpPhase` — the `device_card_ops` progress store — and the device
//! sheets (Name, Troubleshoot, BootloaderEntry) died with the flows that
//! wrote them. The rebuilt model owns activity progress.
//!
//! [`CardTab`]: crate::app::roster::CardTab
//! [identity]: super::ui_sim_card::UiSimCard::identity_key

use crate::app::roster::CardTab;

/// One card's UI view-state. `Default` is a fresh card: the Details front
/// door, nothing open.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CardUiState {
    /// Which tab the user is inspecting.
    pub tab: CardTab,
    /// The open card-resident sheet, if any (D41).
    pub sheet: Option<CardSheet>,
}

/// A card-resident sheet (D41). The confirm arm carries a CORE-TYPED
/// verb, not a web-wired action — the renderer maps verb → concrete
/// action at draw time, so this stays a pure view-state value (float /
/// view-transition renders never see UI internals; e2e can assert it).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CardSheet {
    /// A destructive confirm; the verb names what will run on Confirm.
    Confirm(CardVerb),
}

/// The destructive verb a [`CardSheet::Confirm`] gates. The web maps
/// each to its wired `UiAction` (and confirm copy) at render.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CardVerb {
    /// Stop the simulator session.
    StopSim,
}

/// Mutations to a card's UI view-state, dispatched by the card renderer
/// (tab clicks, sheet opens/closes). Keyed by the card's
/// [`identity_key`](super::ui_sim_card::UiSimCard::identity_key) so
/// the mutation lands on the right runtime regardless of module mode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CardUiOp {
    /// Select a card's tab.
    SelectTab { card: String, tab: CardTab },
    /// Open a card-resident sheet.
    OpenSheet { card: String, sheet: CardSheet },
    /// Close the open sheet.
    CloseSheet { card: String },
}

impl CardUiOp {
    /// The card identity this op targets.
    pub fn card(&self) -> &str {
        match self {
            Self::SelectTab { card, .. }
            | Self::OpenSheet { card, .. }
            | Self::CloseSheet { card } => card,
        }
    }
}
