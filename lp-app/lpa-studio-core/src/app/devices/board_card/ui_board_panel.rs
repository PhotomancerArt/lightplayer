//! [`UiBoardPanel`]: the board's whole panel, drawn in the eight rows' place
//! while the board is connected (the board card ADR, §4 and its 2026-10-19
//! amendment; CD6, CD15).
//!
//! Not a second panel model: its controls are the project's root panel's
//! own ([`super::board_panel_picks`]), each the root panel's
//! [`UiPanelControlView`] with the scope it lives in, so the card draws the
//! panel's widgets and its writes are the panel's own. It holds every
//! control: the card grows once, on connect, to fit them.

use crate::{UiPanelControlState, UiPanelControlView};

use super::ui_board_panel_heading::UiBoardPanelHeading;
use super::ui_card_action::UiCardAction;

/// The board's panel, as the connected card draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct UiBoardPanel {
    /// The root panel's scope target: what the ⋯ menu's panel reset clears
    /// (its details, [`crate::UiDetailPanel::PanelSettings`]).
    pub target: Option<lpc_wire::WireScopeRef>,
    /// Every control on the panel, in panel order: the root's own first
    /// (the band, no heading), then each nested group's under its heading.
    /// Each section's controls are ordered by shape, then the bus's
    /// registry, then the walk ([`crate::panel_order_key`]); nothing is
    /// pulled forward by name.
    pub sections: Vec<UiBoardPanelSection>,
    /// Panel-state auto-save, only at the edit tier (`None` below it): the
    /// ⋯ menu's switch, never drawn on the panel.
    pub auto_save: Option<bool>,
    /// The panel's foot, at the bottom of the card (CD15, Q19): Edit
    /// (`edit`, pencil) at the edit tier, Edit with a lock (`unlock`) on a
    /// play-only board, `None` when neither is offered.
    pub edit: Option<UiCardAction>,
}

impl UiBoardPanel {
    /// Every control, in the order the card draws them.
    pub fn controls(&self) -> impl Iterator<Item = &UiCardControl> {
        self.sections
            .iter()
            .flat_map(|section| section.controls.iter())
    }

    /// How many controls a panel writer holds: what the ⋯ menu's reset
    /// lets go of.
    pub fn held(&self) -> usize {
        self.controls()
            .filter(|control| control.view.state == UiPanelControlState::Engaged)
            .count()
    }
}

/// One run of the panel's controls: the root's own (no heading; the card
/// draws it as the band), or one nested group's (an effect's), under that
/// group's name and kind.
#[derive(Clone, Debug, PartialEq)]
pub struct UiBoardPanelSection {
    /// The nested group's name and kind, drawn as its heading; `None` for
    /// the root's own controls.
    pub heading: Option<UiBoardPanelHeading>,
    pub controls: Vec<UiCardControl>,
}

/// One control, with the scope it lives in (a nested group's control keeps
/// its group's scope: `ModulePanelControl` takes it for the reset and the
/// detail popup).
#[derive(Clone, Debug, PartialEq)]
pub struct UiCardControl {
    pub scope: String,
    pub view: UiPanelControlView,
}
