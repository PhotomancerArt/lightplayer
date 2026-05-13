use crate::ValueSlot;

/// Runtime state exposed by the clock node.
#[derive(lpc_slot_macros::SlotRecord)]
#[slot(root)]
pub struct ClockState {
    /// Clock time in seconds after rate and scrub offset are applied.
    #[slot(produced)]
    pub seconds: ValueSlot<f32>,
    /// Last produced clock delta in seconds.
    #[slot(produced)]
    pub delta_seconds: ValueSlot<f32>,
}

impl Default for ClockState {
    fn default() -> Self {
        Self {
            seconds: ValueSlot::new(0.0),
            delta_seconds: ValueSlot::new(0.0),
        }
    }
}
