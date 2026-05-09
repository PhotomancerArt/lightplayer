//! Graph-level render product handle.
//!
//! A [`RenderProduct`] is the value that moves through produced/consumed slots
//! when a node exposes renderable visual content. It is intentionally small:
//! the engine dispatches materialization requests back to the owning node.

use lpc_model::NodeId;

/// Renderable visual product produced by a node output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderProduct {
    node: NodeId,
    output: u32,
}

impl RenderProduct {
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
