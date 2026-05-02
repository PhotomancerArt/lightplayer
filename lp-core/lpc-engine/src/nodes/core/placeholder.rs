//! Minimal [`crate::node::Node`] stubs for M4 source loading before real core nodes land.

use alloc::boxed::Box;

use lpc_model::FrameId;
use lpc_model::prop::PropPath;
use lpc_source::legacy::nodes::NodeKind;
use lps_shared::LpsValueF32;

use crate::node::{DestroyCtx, MemPressureCtx, Node, NodeError, PressureLevel, TickContext};
use crate::prop::RuntimePropAccess;

#[derive(Default)]
struct EmptyProps;

impl RuntimePropAccess for EmptyProps {
    fn get(&self, _path: &PropPath) -> Option<(LpsValueF32, FrameId)> {
        None
    }

    fn iter_changed_since<'a>(
        &'a self,
        _since: FrameId,
    ) -> Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'a> {
        Box::new(core::iter::empty())
    }

    fn snapshot<'a>(&'a self) -> Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'a> {
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

impl Node for CorePlaceholderNode {
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

    fn props(&self) -> &dyn RuntimePropAccess {
        &self.props
    }
}
