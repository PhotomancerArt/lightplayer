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

use crate::app::device::BootloaderEntryFlow;
use crate::app::roster::DeviceCardTab;

/// One card's UI view-state. `Default` is a fresh card: the Settings
/// front door,
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
    /// The setup form's board choice (`vendor/product`, board-selection
    /// M5). `None` = the generic-board fallback. Core-owned for the same
    /// reason as the tab: the pick must survive the card re-rendering
    /// (every view tick) and the tab being toggled away mid-setup.
    pub setup_board: Option<String>,
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
    /// The Not-responding card's troubleshooting sheet (M6).
    Troubleshoot,
    /// The bootloader-entry ritual: steps → waiting → confirmation.
    ///
    /// Carries the whole [`BootloaderEntryFlow`] as a value, so advancing
    /// the ritual is just re-opening the sheet with the next state — no
    /// separate op vocabulary, and the flow stays inspectable by e2e.
    ///
    /// Core ownership matters more here than for any other sheet: the
    /// ritual REQUIRES physically unplugging the device, which tears down
    /// the session and re-renders the card. View-state that died with the
    /// component would lose the user's place at exactly the moment the
    /// confirmation is supposed to arrive.
    BootloaderEntry(BootloaderEntryFlow),
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
    /// Wipe the device's project storage back to blank (the
    /// Holds-unreadable-data card's way out — model rev 2026-07-26).
    WipeProject,
}

/// The in-place progress of a heavy op running on a card — a CARD-OWNED
/// flow, not a session attribute (state-flow model §2, ratified
/// 2026-07-26). The controller sets it at op dispatch and clears it at
/// landing; the live session only FEEDS progress. The session dying does
/// NOT clear it (invariant I1) — heavy ops sever the very session that
/// used to own their progress, which was the erase dead-end class.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardOp {
    /// The human label, e.g. "Installing firmware…".
    pub label: String,
    /// Completion percent when the op reports one; `None` = indeterminate.
    pub percent: Option<u8>,
    /// Where the flow is (model §2): running, riding out an EXPECTED
    /// disconnect, or failed.
    pub phase: CardOpPhase,
}

/// The op flow's phase (model §2).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum CardOpPhase {
    /// The link op is executing; progress may tick.
    #[default]
    Running,
    /// The op's EXPECTED disconnect happened (reboot / re-enumeration);
    /// the reconnect ladder is armed and the overlay stays up (I2).
    AwaitingDevice,
    /// Terminal until the user takes the ONE exit to the nearest stable
    /// state (I4) — e.g. "Back to set up" after a failed install.
    Failed {
        /// What went wrong, for the overlay's terminal region.
        error: String,
        /// The exit button's label (the nearest stable state's door).
        exit_label: String,
    },
}

impl CardOp {
    /// A running op (the dispatch-time constructor).
    pub fn new(label: impl Into<String>, percent: Option<u8>) -> Self {
        Self {
            label: label.into(),
            percent,
            phase: CardOpPhase::Running,
        }
    }

    /// The expected-disconnect phase: same flow, new label (e.g.
    /// "Restarting…" / "Waiting for the board…"), indeterminate.
    pub fn awaiting(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            percent: None,
            phase: CardOpPhase::AwaitingDevice,
        }
    }

    /// The failed terminal: label + error + the single exit's label.
    pub fn failed(
        label: impl Into<String>,
        error: impl Into<String>,
        exit_label: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            percent: None,
            phase: CardOpPhase::Failed {
                error: error.into(),
                exit_label: exit_label.into(),
            },
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
    /// Take a FAILED op's single exit (model §2 I4): clears the op flow so
    /// the card re-derives its honest state ("Back to set up" lands the
    /// blank board on the setup form).
    ClearOp { card: String },
    /// Pick the setup form's board (`None` = generic fallback).
    SelectSetupBoard {
        card: String,
        board_id: Option<String>,
    },
}

impl CardUiOp {
    /// The card identity this op targets.
    pub fn card(&self) -> &str {
        match self {
            Self::SelectTab { card, .. }
            | Self::OpenSheet { card, .. }
            | Self::CloseSheet { card }
            | Self::ClearOp { card }
            | Self::SelectSetupBoard { card, .. } => card,
        }
    }
}
