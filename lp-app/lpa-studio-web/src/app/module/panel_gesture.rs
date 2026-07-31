//! Panel gestures the spike's surfaces raise.
//!
//! **M2 UX spike seam.** `docs/design/panel.md` P8 says the wire carries
//! exactly two runtime ops — `PanelWrite { scope, channel, value }` and
//! `PanelClear { scope?, channel? }` — and that Studio, play mode, phones,
//! and future hardware inputs all speak only those. Neither op exists yet
//! (M4 owns the engine runtime), so the spike's components raise this
//! component-level gesture instead and the stories fake the resulting
//! state. The variants are deliberately shaped like the two wire ops so
//! the mapping at implementation time is mechanical:
//!
//! - [`PanelGesture::ClearControl`] → `PanelClear { scope, channel }`
//! - [`PanelGesture::ClearScope`] → `PanelClear { scope }`
//! - [`PanelGesture::SetAutoSave`] → the P11 auto-save toggle
//! - [`PanelGesture::ToggleGroup`] is pure disclosure — it has no wire op
//!   and never will; it is view state (a `CardUiState` sibling at M4).
//!
//! Panel *writes* are NOT here: a knob drag still rides the existing
//! `SlotEditOp::SetValue` path so the spike reuses knob v2 untouched. That
//! is a spike shortcut, not a proposal — see the module face's doc.

/// One gesture raised by a panel surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PanelGesture {
    /// Clear one control's panel writer, returning it to Read (P2).
    ClearControl {
        /// The scope the control lives in.
        scope: String,
        /// The channel name within that scope.
        channel: String,
    },
    /// Clear every panel writer under one scope — the per-module reset.
    /// The lean (P-Q4) is that this descends into nested groups.
    ClearScope {
        /// The scope to clear.
        scope: String,
    },
    /// Collapse or expand a nested panel group. Disclosure only.
    ToggleGroup {
        /// The nested group's scope.
        scope: String,
    },
    /// Flip panel-state auto-save (P11 — on by default).
    SetAutoSave(bool),
}
