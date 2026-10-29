//! What a layout is given: the nodes, the dataflow between them, and the
//! pins. Owned and comparable, so a caller can keep the last one and lay
//! out again only when it changes.

use crate::{LayoutNode, Vec2};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayoutInput {
    /// Every node, in authoring order: siblings keep this order wherever
    /// the rules leave a choice.
    pub nodes: Vec<LayoutNode>,
    /// Dataflow, `(from, to)`: a shader feeding a fixture, a fixture
    /// feeding an output. It orders the columns and places the outputs.
    pub edges: Vec<(String, String)>,
    /// Fixed points: a node's top-left in its parent's inside
    /// coordinates. Empty while pinning is off (D64).
    pub pins: Vec<(String, Vec2)>,
}

impl LayoutInput {
    pub fn new(nodes: Vec<LayoutNode>) -> Self {
        Self {
            nodes,
            ..Self::default()
        }
    }

    /// The same input with an edge from `from` to `to`.
    pub fn edge(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.edges.push((from.into(), to.into()));
        self
    }

    /// The same input with `id` pinned at `at`.
    pub fn pin(mut self, id: impl Into<String>, at: Vec2) -> Self {
        self.pins.push((id.into(), at));
        self
    }
}
