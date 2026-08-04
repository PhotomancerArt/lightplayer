use crate::{NodeId, Slotted, TimeProduct, TimeProductSlot, ValueSlot};

/// Runtime state exposed by the clock node.
#[derive(Slotted)]
#[slot(default_role = "state")]
pub struct ClockState {
    /// The queryable timebase this clock owns, published on `bus:time`.
    ///
    /// The bus carries the **product**, never raw seconds: readers query it
    /// for seconds, delta, and phasors. `seconds`/`delta_seconds` below stay
    /// produced-but-unbound so the card face and probes can still read the
    /// plain numbers — binding both to `time` would put two fallback
    /// producers on one channel (`AmbiguousBusBinding`).
    #[slot(produced, default_bind = "bus:time")]
    pub product: TimeProductSlot,
    /// Clock time in seconds after rate and scrub offset are applied.
    #[slot(produced)]
    pub seconds: ValueSlot<f32>,
    /// Last produced clock delta in seconds.
    #[slot(produced)]
    pub delta_seconds: ValueSlot<f32>,
}

impl ClockState {
    /// State for the clock attached to `node`, with its time product handle
    /// seeded to that node's first output (module-mirror precedent: the
    /// published handle names its own node and never changes).
    #[must_use]
    pub fn for_node(node: NodeId) -> Self {
        Self {
            product: TimeProductSlot::new(TimeProduct::new(node, 0)),
            ..Self::default()
        }
    }
}

impl Default for ClockState {
    fn default() -> Self {
        Self {
            product: TimeProductSlot::default(),
            seconds: ValueSlot::new(0.0),
            delta_seconds: ValueSlot::new(0.0),
        }
    }
}
