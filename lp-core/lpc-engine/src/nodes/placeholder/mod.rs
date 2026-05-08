//! Minimal [`crate::node::NodeRuntime`] stubs for M4 source loading before real core nodes land.

use alloc::boxed::Box;

use lpc_model::NodeKind;
use lpc_model::Revision;
use lpc_model::SlotPath;

use crate::node::{DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, TickContext};
use crate::prop::ProducedSlotAccess;
use crate::runtime_product::RuntimeProduct;

#[derive(Default)]
struct EmptyProps;

impl ProducedSlotAccess for EmptyProps {
    fn get(&self, _path: &SlotPath) -> Option<(RuntimeProduct, Revision)> {
        None
    }

    fn iter_changed_since<'a>(
        &'a self,
        _since: Revision,
    ) -> Box<dyn Iterator<Item = (SlotPath, RuntimeProduct, Revision)> + 'a> {
        Box::new(core::iter::empty())
    }

    fn snapshot<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (SlotPath, RuntimeProduct, Revision)> + 'a> {
        Box::new(core::iter::empty())
    }
}

/// Placeholder runtime node used while wiring source load into the core tree.
pub struct CorePlaceholderNode {
    /// `None` for synthetic spine folders (`*.folder` segments).
    pub kind: Option<NodeKind>,
    props: EmptyProps,
}

impl CorePlaceholderNode {
    pub fn new_folder() -> Self {
        Self {
            kind: None,
            props: EmptyProps,
        }
    }

    pub fn new_leaf(kind: NodeKind) -> Self {
        Self {
            kind: Some(kind),
            props: EmptyProps,
        }
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

    fn produced(&self) -> &dyn ProducedSlotAccess {
        &self.props
    }
}
