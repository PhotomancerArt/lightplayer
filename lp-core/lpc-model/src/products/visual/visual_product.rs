//! Graph-level visual product handle.
//!
//! A [`VisualProduct`] is the value that moves through node slots when a node
//! exposes renderable visual content. It is intentionally small: the engine
//! dispatches materialization requests back to the owning runtime node.

use crate::NodeId;

/// Renderable visual product produced by a node output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct VisualProduct {
    node: NodeId,
    output: u32,
}

impl VisualProduct {
    #[must_use]
    pub const fn new(node: NodeId, output: u32) -> Self {
        Self { node, output }
    }

    #[must_use]
    pub const fn node(self) -> NodeId {
        self.node
    }

    #[must_use]
    pub const fn output(self) -> u32 {
        self.output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_product_keeps_owner_and_output() {
        let product = VisualProduct::new(NodeId::new(7), 2);

        assert_eq!(product.node(), NodeId::new(7));
        assert_eq!(product.output(), 2);
    }
}
