//! The clock tape's per-leaf wiring: one wire per transport dimension.

use crate::{ProjectSlotAddress, UiPanelTarget};

/// Which dimension of the clock's transport one wire drives.
///
/// Deliberately a small closed set rather than an open registry: the
/// clock's tape is the one instrument whose gestures land on several
/// channels, and a match arm that has to be widened is a better signal than
/// a lookup that silently accepts anything.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum UiPanelWireRole {
    /// The transport's speed multiplier (the fader) — `clock.rate`.
    Rate,
    /// The transport's run/pause setpoint — `clock.play_state`.
    PlayState,
    /// The transport's scrub offset in signed seconds — `clock.scrub`.
    Scrub,
}

/// One DIMENSION of the clock tape's wiring.
///
/// The tape on the clock's own card puts a speed fader, a run/pause button
/// and a scrub strip on one faceplate, and each dimension dispatches on its
/// own (`2026-08-07-clock-transport-is-a-panel-instrument.md` §3):
///
/// - the faceplate always renders whole (rendering is a shape fact);
/// - a gesture on a wire carrying a [`Self::panel_target`] is a
///   `PanelWriteOp` on THAT channel, and one without falls back to a slot
///   edit at THAT wire's [`Self::address`] (dispatch is a per-leaf fact).
///
/// On a module panel the transport is one control, the rate's Speed knob
/// ([`crate::UiPanelWidget::Speed`]), which carries its channel the way any
/// one-channel control does; these wires are the card's
/// ([`crate::UiClockFace::wires`]).
#[derive(Clone, Debug, PartialEq)]
pub struct UiPanelWire {
    /// Which dimension of the transport this wire drives.
    pub role: UiPanelWireRole,
    /// Slot-edit fallback address for this dimension; `None` when its row
    /// is not writable.
    pub address: Option<ProjectSlotAddress>,
    /// The `(scope, channel)` a gesture on this dimension writes, present
    /// exactly when this leaf's wiring is panel-public.
    pub panel_target: Option<UiPanelTarget>,
    /// This dimension's channel reading, already quantized — the echo a
    /// panel write on this channel comes back through.
    pub live_value: Option<String>,
}
