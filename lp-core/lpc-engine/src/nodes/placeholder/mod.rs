//! Minimal [`crate::node::NodeRuntime`] for projected nodes without behavior.

use alloc::format;
use lpc_model::{LpFeature, NodeKind, NodeRuntimeStatus};

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

    /// The build-gap message for a gated-out kind, in one place so the tree
    /// status and the resolve error say the same thing.
    pub fn unsupported_kind_message(kind: NodeKind) -> alloc::string::String {
        format!("node kind {kind:?} is not included in this firmware build")
    }
}

impl NodeRuntime for CorePlaceholderNode {
    /// A placeholder standing in for a GATEABLE kind reports the build gap.
    ///
    /// Two other placeholder shapes must stay silent: the synthetic spine
    /// folder (`kind: None`), and the project ROOT — spelled
    /// `new_leaf(NodeKind::Project)`, an always-on kind whose placeholder
    /// means "the root has no behavior", never "this build lacks Project".
    /// [`LpFeature::for_node_kind`] draws exactly that line: a kind with no
    /// feature can never be gated out.
    fn runtime_status(&self) -> Option<NodeRuntimeStatus> {
        let kind = self.kind?;
        LpFeature::for_node_kind(kind)
            .map(|_| NodeRuntimeStatus::Unsupported(Self::unsupported_kind_message(kind)))
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
