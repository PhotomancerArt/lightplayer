//! Front-panel control widgets for node card faces.
//!
//! The panel is the face's gesture surface: knobs, faders, and toggles
//! projected from panel-flagged slots ([`lpa_studio_core::UiPanelControl`]).
//! Fields follow the stateless slot-field pattern (`value, state, address,
//! on_action`) and dispatch through the standard slot write path
//! (`SlotEditOp::SetValue`), so drags coalesce exactly like slot editor
//! floods. [`PanelControl`] is the shared chrome: the state-colored label
//! button (name + info glyph — the detail trigger, opening the SAME
//! popover as the backing slot row) and the value + unit readout.

mod h_fader_field;
mod knob_field;
mod panel_control;
mod panel_emit;
mod toggle_field;

pub use h_fader_field::HFaderField;
pub use knob_field::KnobField;
/// Story fixtures snap their values through the widget's own rule so the
/// story stays a faithful record of what the app renders.
#[cfg(feature = "stories")]
pub(crate) use knob_field::knob_snap;
pub use panel_control::PanelControl;
pub use panel_emit::PanelEmit;
pub use toggle_field::ToggleField;
