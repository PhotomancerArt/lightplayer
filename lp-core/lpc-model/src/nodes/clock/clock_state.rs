use crate::{Slotted, ValueSlot};

/// Runtime state exposed by the clock node.
#[derive(Slotted)]
#[slot(default_policy = "read_only_transient")]
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
