//! Graph-level time product handle.
//!
//! A [`TimeProduct`] is the value that moves through node slots when a node
//! exposes a queryable timebase (seconds, delta, phasor state). It is
//! intentionally small: the engine services timebase queries from a side
//! store keyed by the owning node, not by dispatching back into the graph.

use crate::NodeId;

/// Queryable timebase product produced by a node output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct TimeProduct {
    node: NodeId,
    output: u32,
}

impl TimeProduct {
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
    fn time_product_keeps_owner_and_output() {
        let product = TimeProduct::new(NodeId::new(7), 2);

        assert_eq!(product.node(), NodeId::new(7));
        assert_eq!(product.output(), 2);
    }
}
