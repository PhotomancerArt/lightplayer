//! Minimal [`crate::node::NodeRuntime`] stubs for M4 source loading before real core nodes land.

use lpc_model::NodeKind;

use crate::node::{DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, TickContext};

/// Placeholder runtime node used while wiring source load into the core tree.
pub struct CorePlaceholderNode {
    /// `None` for synthetic spine folders (`*.folder` segments).
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
    fn tick(&mut self, _ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        Ok(())
    }

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
