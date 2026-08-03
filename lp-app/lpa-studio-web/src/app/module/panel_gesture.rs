//! Panel gestures the module surfaces raise.
//!
//! `docs/design/panel.md` P8: the wire carries exactly two runtime ops —
//! `PanelWrite { scope, channel, value }` and `PanelClear { scope?,
//! channel? }` — and Studio, play mode, phones, and future hardware inputs
//! all speak only those. Widget VALUE writes don't come through here: the
//! panel widgets carry the control's `UiPanelTarget` and dispatch
//! `PanelWriteOp` themselves (the same `panel_or_slot_action` path the
//! node-card controls use). What's left is the clear/reset family and the
//! P11 auto-save toggle, raised as a component-level gesture and translated
//! into `UiAction`s at ONE seam ([`panel_gesture_actions`]) so the panel
//! components stay dispatch-agnostic and the stories can fake state by
//! intercepting the same events. Both mounts of a panel — the node card's
//! face body and the play-mode surface — go through that one translator.
//!
//! There is no group-disclosure gesture: nested groups are bordered
//! clusters in a wrapping row and never collapse, so there is no view
//! state to carry.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiPanelTarget};

use crate::app::node::slot_edit_actions::{
    panel_auto_save_action, panel_clear_action, panel_clear_scope_action,
};

/// One gesture raised by a panel surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PanelGesture {
    /// Clear one control's panel writer, returning it to Read (P2). The
    /// target carries the `(scope, channel)` identity verbatim.
    ClearControl {
        /// The control's `(scope, channel)` write target.
        target: UiPanelTarget,
    },
    /// Clear every panel writer under one scope — the per-module reset.
    /// Descends into nested groups (settled P-Q4).
    ClearScope {
        /// The scope to clear.
        scope: lpc_wire::WireScopeRef,
    },
    /// Flip panel-state auto-save (P11 — on by default).
    SetAutoSave(bool),
}

/// The gesture→op seam (panel.md P8): every gesture becomes a wire-shaped
/// op, riding the given action dispatcher. Every live mount of a panel
/// surface builds its handler here, so Studio and play mode speak exactly
/// the same ops.
pub fn panel_gesture_actions(on_action: EventHandler<UiAction>) -> EventHandler<PanelGesture> {
    EventHandler::new(move |gesture: PanelGesture| match gesture {
        PanelGesture::ClearControl { target } => {
            on_action.call(panel_clear_action(&target));
        }
        PanelGesture::ClearScope { scope } => {
            on_action.call(panel_clear_scope_action(scope));
        }
        PanelGesture::SetAutoSave(enabled) => {
            on_action.call(panel_auto_save_action(enabled));
        }
    })
}
