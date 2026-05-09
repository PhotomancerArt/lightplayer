//! Public runtime state shape for shader nodes.

use crate::{RenderProduct, RenderProductSlot};

/// Runtime state exposed by a shader node.
#[derive(lpc_slot_macros::SlotRecord)]
#[slot(root)]
pub struct ShaderState {
    /// Renderable visual output produced by this shader node.
    pub output: RenderProductSlot,
}

impl ShaderState {
    pub fn new(output: RenderProduct) -> Self {
        Self {
            output: RenderProductSlot::new(output),
        }
    }
}
