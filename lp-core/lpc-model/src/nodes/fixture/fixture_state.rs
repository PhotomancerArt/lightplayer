//! Public runtime state shape for fixture nodes.

use crate::{ControlExtent, ControlProduct, ControlProductSlot, NodeId, Slotted};

/// Runtime state exposed by a fixture node.
#[derive(Default, Slotted)]
#[slot(default_policy = "read_only_transient")]
pub struct FixtureState {
    /// Renderable control output produced by this fixture node.
    #[slot(produced, default_bind = "bus:control.out")]
    pub output: ControlProductSlot,
}

impl FixtureState {
    pub fn new(node: NodeId, output: u32, preferred_extent: ControlExtent) -> Self {
        Self {
            output: ControlProductSlot::new(ControlProduct::new(node, output, preferred_extent)),
        }
    }
}
