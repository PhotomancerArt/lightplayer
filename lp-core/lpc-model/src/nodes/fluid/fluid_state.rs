//! Public runtime state shape for fluid nodes.

use crate::{VisualProduct, VisualProductSlot};

/// Runtime state exposed by a fluid node.
#[derive(lpc_slot_macros::SlotRecord)]
#[slot(root)]
pub struct FluidState {
    /// Renderable visual output produced by this fluid node.
    #[slot(produced)]
    pub output: VisualProductSlot,
}

impl FluidState {
    pub fn new(output: VisualProduct) -> Self {
        Self {
            output: VisualProductSlot::new(output),
        }
    }
}
