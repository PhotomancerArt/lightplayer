//! Public runtime state shape for fixture nodes.

use crate::{ControlExtent, ControlProduct, ControlProductSlot, NodeId};

/// Runtime state exposed by a fixture node.
#[derive(lpc_slot_macros::SlotRecord)]
#[slot(root, default_policy = "read_only_transient")]
pub struct FixtureState {
    /// Renderable control output produced by this fixture node.
    pub output: ControlProductSlot,
}

impl FixtureState {
    pub fn new(node: NodeId, output: u32, preferred_extent: ControlExtent) -> Self {
        Self {
            output: ControlProductSlot::new(ControlProduct::new(node, output, preferred_extent)),
        }
    }
}
