//! Public runtime state shape for fluid nodes.

use crate::{Slotted, VisualProduct, VisualProductSlot};

/// Runtime state exposed by a fluid node.
#[derive(Default, Slotted)]
#[slot(default_policy = "read_only_transient")]
pub struct FluidState {
    /// Renderable visual output produced by this fluid node.
    #[slot(produced, default_bind = "bus:visual.out")]
    pub output: VisualProductSlot,
}

impl FluidState {
    pub fn new(output: VisualProduct) -> Self {
        Self {
            output: VisualProductSlot::new(output),
        }
    }
}
