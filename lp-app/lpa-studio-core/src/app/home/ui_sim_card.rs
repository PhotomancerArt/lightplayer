//! The live simulator session as the gallery's roster shows it.
//!
//! This was `UiDeviceCard` — one card type serving devices AND the sim.
//! The device half went with M2 of the device-model rebuild (the rebuilt
//! model projects its own card DTO); what is left is the sim's card, which
//! shared the type by accident rather than by need. The card grammar
//! itself (title bar, icon tabs, sheets, the ▶ play tab) is unchanged.

use crate::UiLogEntry;
use crate::app::home::card_ui_state::CardUiState;
use crate::app::node::UiControlProductPreview;
use crate::app::roster::SimCardState;

/// The live simulator's card. Visually distinct from package cards by
/// contract: the renderer gives it a runtime header (sim glyph + status) so
/// it never reads as "just another project". The card's health lives in
/// [`SimCardState`]; the project chip is identity, not status.
///
/// The sim is not a device (D22): no uid, no registry entry, no transport,
/// no firmware provenance — and the card exists only while the session
/// does.
#[derive(Clone, Debug, PartialEq)]
pub struct UiSimCard {
    /// Where the card stands (running a project, or empty).
    pub state: SimCardState,
    /// The project the sim runs — identity for the card's ▶ tab (the
    /// project chip on the play tab's meta row), never health. `None` means
    /// no ▶ tab at all: nothing to draw.
    pub project: Option<UiSimProjectChip>,
    /// The board this session claims to be (`vendor/product`,
    /// gallery-rework vision D4), rendered as the card's "as \<board\>"
    /// line.
    ///
    /// Advisory context ONLY, and INHERITED from the project the sim runs:
    /// load-as-push sets it from that project's manifest `target`, so the
    /// persisted fact lives in `project.json` and is re-derived on every
    /// load (the sim itself persists nothing — D22). `None` — no board
    /// known — is the ordinary default.
    pub board_id: Option<String>,
    /// The session's console tail (D42), oldest first — the card's console
    /// strip and Console tab render this. It dies with the session (the
    /// console is the session's, not the app's).
    pub console_tail: Vec<UiLogEntry>,
    /// The newest frame the SIM ENGINE published, as the ▶ Play tab draws
    /// it: the running control product read off the simulated board, never
    /// re-simulated a second time in the browser.
    pub frame_preview: Option<UiControlProductPreview>,
    /// How many seconds old [`Self::frame_preview`] is, stamped at view
    /// build against the studio clock. The stale treatment engages past
    /// [`FRAME_STALE_AFTER_SECS`](crate::FRAME_STALE_AFTER_SECS); `None`
    /// exactly when there is no frame.
    pub frame_age_secs: Option<f64>,
    /// The sim engine's reported fps, when the runtime reports one.
    pub frame_fps: Option<f32>,
    /// The card's UI view-state (selected tab, open sheet). Core-owned +
    /// keyed by [`Self::identity_key`], so it survives the card ⇄ pane
    /// growth. The gallery/lens builder leaves this default; the controller
    /// overlays the persisted state.
    pub ui: CardUiState,
}

/// The (≤1) sim card's reserved identity key — the sim has no uid and no
/// registry entry, so its `CardUiState` and view-transition name key by
/// this token instead. Named because the controller's default-tab rule has
/// to recognize the sim card by key alone.
pub const SIM_CARD_KEY: &str = "runtime-sim";

impl UiSimCard {
    /// The card's CANONICAL identity — the ONE key both the UI-state map
    /// and the scene-fork's `view-transition-name` consume (2026-07-25
    /// alignment). The (≤1) sim card keys by a reserved token: names are
    /// NOT unique, and a keyed list with duplicate keys panics Dioxus (the
    /// 2026-07-15 crash).
    pub fn identity_key(&self) -> &str {
        SIM_CARD_KEY
    }

    /// Back-compat alias for keyed rendering — the same canonical key.
    pub fn render_key(&self) -> &str {
        self.identity_key()
    }
}

/// The sim's project, as the card's ▶ tab names it: thumbnail seed +
/// display name. Identity only — the status line and edge tint carry
/// health.
#[derive(Clone, Debug, PartialEq)]
pub struct UiSimProjectChip {
    /// `prj…` uid — thumbnail seed and the project-card pairing key.
    pub uid: String,
    /// Display name (library slug; a deleted project falls back to uid).
    pub name: String,
}
