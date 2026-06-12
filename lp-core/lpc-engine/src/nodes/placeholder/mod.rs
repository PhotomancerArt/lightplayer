//! Minimal [`crate::node::NodeRuntime`] for projected nodes without behavior.

use lpc_model::NodeKind;

use crate::node::{DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel};

/// Runtime placeholder for synthetic projection nodes and load-error entries.
///
/// Project projection sometimes needs a runtime tree entry before there is a
/// concrete behavior to attach, or for a node whose definition is currently in
/// an error state. This node keeps those entries addressable without producing
/// values of its own.
pub struct CorePlaceholderNode {
    /// `None` for synthetic spine folders.
    pub kind: Option<NodeKind>,
}

impl CorePlaceholderNode {
    pub fn new_folder() -> Self {
        Self { kind: None }
    }

    pub fn new_leaf(kind: NodeKind) -> Self {
        Self { kind: Some(kind) }
    }
}

impl NodeRuntime for CorePlaceholderNode {
    fn destroy(&mut self, _ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError> {
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: PressureLevel,
        _ctx: &mut MemPressureCtx<'_>,
    ) -> Result<(), NodeError> {
        Ok(())
    }
}
