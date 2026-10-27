//! Active engine call vocabulary for node execution.

use alloc::format;
use alloc::string::String;

use lpc_model::{NodeId, SlotPath};

use crate::products::control::ControlProduct;
use crate::products::visual::VisualProduct;

/// A kind of engine-dispatched call into a runtime node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeCall {
    Tick,
    /// The resolver's interned handle for the produced path: a dispatch
    /// copies a pointer, not the path.
    ProduceSlot {
        slot: alloc::rc::Rc<SlotPath>,
    },
    Visual {
        product: VisualProduct,
    },
    Control {
        product: ControlProduct,
    },
}

impl NodeCall {
    pub fn label(&self) -> String {
        match self {
            Self::Tick => String::from("tick"),
            Self::ProduceSlot { slot } => format!("produce slot {slot:?}"),
            Self::Visual { product } => format!("render output {}", product.output()),
            Self::Control { product } => {
                format!("render control output {}", product.output())
            }
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
