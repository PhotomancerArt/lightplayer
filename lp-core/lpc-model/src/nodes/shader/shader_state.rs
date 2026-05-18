//! Public runtime state shape for shader nodes.

use crate::{Slotted, VisualProduct, VisualProductSlot};

/// Runtime state exposed by a shader node.
#[derive(Default, Slotted)]
pub struct ShaderState {
    /// Renderable visual output produced by this shader node.
    pub output: VisualProductSlot,
}

impl ShaderState {
    pub fn new(output: VisualProduct) -> Self {
        Self {
            output: VisualProductSlot::new(output),
        }
    }
}
