//! [`UiBoardPanel`]: the board's panel at card size, drawn in the five bars'
//! place while the board is connected (the board card ADR, §4; CD6, CD15).
//!
//! Not a second panel model: its controls are picked from the project's
//! root panel ([`super::board_panel_picks`]), each the root panel's own
//! [`UiPanelControlView`] with the scope it lives in, so the card draws the
//! panel's widgets and its writes are the panel's own. What the card leaves
//! out is the play page's (All controls): `more` says how much.

use crate::UiPanelControlView;

use super::ui_card_action::UiCardAction;

/// The board's panel, as the connected card draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct UiBoardPanel {
    /// The root panel's scope target (its clears and resets).
    pub target: Option<lpc_wire::WireScopeRef>,
    /// The controls the card draws, in order: the master first.
    pub controls: Vec<UiCardControl>,
    /// How many more the board's own page has.
    pub more: usize,
    /// Panel-state auto-save, only at the edit tier (`None` below it).
    pub auto_save: Option<bool>,
    /// The All controls row's action, flush at its end (CD15, Q19): Edit
    /// (`edit`, pencil) at the edit tier, Edit with a lock (`unlock`) on a
    /// play-only board, `None` when neither is offered.
    pub edit: Option<UiCardAction>,
}

/// One picked control, with the scope it lives in (a nested group's
/// control keeps its group's scope: `ModulePanelControl` takes it for the
/// reset and the detail popup).
#[derive(Clone, Debug, PartialEq)]
pub struct UiCardControl {
    pub scope: String,
    pub view: UiPanelControlView,
}
