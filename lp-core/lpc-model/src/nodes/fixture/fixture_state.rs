//! Public runtime state shape for fixture nodes.

use crate::{ControlExtent, ControlProduct, ControlProductSlot, NodeId, Slotted, ValueSlot};

/// Runtime state exposed by a fixture node.
#[derive(Slotted)]
#[slot(default_policy = "read_only_transient")]
pub struct FixtureState {
    /// Renderable control output produced by this fixture node.
    #[slot(produced, default_bind = "bus:control.out")]
    pub output: ControlProductSlot,
    /// Estimated draw of the last rendered frame, in milliamps.
    ///
    /// From the lamp type's power model — an estimate, never a measurement.
    /// Zero when the fixture declares no power budget.
    #[slot(produced)]
    pub estimated_draw_ma: ValueSlot<u32>,
    /// Output scale currently imposed by current limiting, `0.0..=1.0`.
    ///
    /// `1.0` means nothing is being shed: either the fixture is inside its
    /// budget or it has none. A value below `1.0` is the only thing that tells
    /// deliberate limiting apart from a project that is simply dim.
    #[slot(produced)]
    pub power_scale: ValueSlot<f32>,
    /// The budget actually in force, in milliamps, after an unstated one has
    /// fallen back to the default. Zero when limiting was opted out of.
    ///
    /// Published rather than left for readers to re-derive: the defaulting rule
    /// lives in one place, and a reader that guessed differently would report a
    /// percentage against a budget nothing is enforcing.
    #[slot(produced)]
    pub power_budget_ma: ValueSlot<u32>,
}

impl Default for FixtureState {
    fn default() -> Self {
        Self {
            output: ControlProductSlot::default(),
            estimated_draw_ma: ValueSlot::new(0),
            power_scale: ValueSlot::new(1.0),
            power_budget_ma: ValueSlot::new(0),
        }
    }
}

impl FixtureState {
    pub fn new(node: NodeId, output: u32, preferred_extent: ControlExtent) -> Self {
        Self {
            output: ControlProductSlot::new(ControlProduct::new(node, output, preferred_extent)),
            ..Self::default()
        }
    }
}
