//! Active engine call vocabulary for node execution.

use alloc::format;
use alloc::string::String;

use lpc_model::{NodeId, SlotPath};

use crate::render_product::RenderProduct;

/// A kind of engine-dispatched call into a runtime node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeCall {
    Tick,
    ProduceSlot { slot: SlotPath },
    Render { product: RenderProduct },
}

impl NodeCall {
    pub fn label(&self) -> String {
        match self {
            Self::Tick => String::from("tick"),
            Self::ProduceSlot { slot } => format!("produce slot {slot:?}"),
            Self::Render { product } => format!("render output {}", product.output()),
        }
    }
}

/// Concrete active call against a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeCallKey {
    pub node: NodeId,
    pub call: NodeCall,
}

impl NodeCallKey {
    #[must_use]
    pub const fn new(node: NodeId, call: NodeCall) -> Self {
        Self { node, call }
    }
}
