//! The device card's UI VIEW-STATE — core-owned (2026-07-25 re-home).
//!
//! What the user is inspecting on a card (the selected [`DeviceCardTab`]),
//! any card-resident flow that's open (a [`CardSheet`]), and the in-place
//! progress of a heavy operation ([`CardOp`]). This used to live in the
//! web renderer's `use_signal`s, which meant it died when a card's
//! component instance did — amnesia across the card ⇄ pane growth (D43),
//! and unreachable by e2e (tests could only dispatch the underlying op,
//! never drive open→confirm). Core ownership fixes both: the state
//! survives module mode changes and is drivable past the dispatch
//! boundary.
//!
//! The state is keyed by a card's canonical [identity] so it follows the
//! device, not the widget: growing a card into the editor pane, or a
//! session replace, keeps the same tab and any open sheet.
//!
//! [`DeviceCardTab`]: crate::app::roster::DeviceCardTab
//! [identity]: super::ui_device_card::UiDeviceCard::identity_key

use crate::app::roster::DeviceCardTab;

/// One card's UI view-state. `Default` is a fresh card: the Status tab,
/// nothing open, no op in flight.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CardUiState {
    /// Which tab the user is inspecting.
    pub tab: DeviceCardTab,
    /// The open card-resident sheet, if any (D41).
    pub sheet: Option<CardSheet>,
    /// The in-place progress of a heavy op (flash / erase / reset),
    /// when one is running on this card. While set, the card renders the
    /// progress overlay (blur + cover the tabs) instead of its tab body.
    pub op: Option<CardOp>,
}

/// A card-resident sheet (D41). The confirm arm carries a CORE-TYPED
/// verb, not a web-wired action — the renderer maps verb → concrete
/// action at draw time, so this stays a pure view-state value (float /
/// view-transition renders never see UI internals; e2e can assert it).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CardSheet {
    /// A destructive confirm; the verb names what will run on Confirm.
    Confirm(CardVerb),
    /// The name-stamping sheet (an unstamped board — retired for the
    /// one-click provision flow, kept for explicit re-stamp paths).
    Name,
    /// The D30 drift-resolution sheet (adopt / keep-both / stay).
    Drift,
    /// The Not-responding card's troubleshooting sheet (M6).
    Troubleshoot,
}

/// The destructive verb a [`CardSheet::Confirm`] gates. The web maps
/// each to its wired `UiAction` (and confirm copy) at render.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CardVerb {
    /// Erase the device's flash entirely.
    Erase,
    /// Forget a remembered (offline) device.
    Forget,
    /// Stop the simulator session.
    StopSim,
    /// Push a dropped project onto the device (drag-onto-card).
    PushDrop { key: String },
    /// Flash / reflash firmware onto a live board (the destructive,
    /// already-provisioned path — a blank board flashes with no confirm).
    Flash,
}

/// The in-place progress of a heavy op running on a card. The card
/// renders a bar + this label + the session's console tail (the
/// technical terminal); no separate wire — it mirrors the session's
/// `operation_label`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardOp {
    /// The human label, e.g. "Installing firmware…".
    pub label: String,
    /// Completion percent when the op reports one; `None` = indeterminate.
    pub percent: Option<u8>,
}

impl CardOp {
    pub fn new(label: impl Into<String>, percent: Option<u8>) -> Self {
        Self {
            label: label.into(),
            percent,
        }
    }
}

/// Mutations to a card's UI view-state, dispatched by the card renderer
/// (tab clicks, sheet opens/closes). Keyed by the card's
/// [`identity_key`](super::ui_device_card::UiDeviceCard::identity_key) so
/// the mutation lands on the right device regardless of module mode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CardUiOp {
    /// Select a card's tab.
    SelectTab { card: String, tab: DeviceCardTab },
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
