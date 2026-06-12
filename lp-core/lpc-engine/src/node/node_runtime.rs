//! Engine spine [`NodeRuntime`] trait: produce, consume, destroy, memory pressure, and runtime state.

use crate::resource::RuntimeBufferId;
use lpc_model::{SlotAccess, SlotPath, SlotShapeRegistry, SlotShapeRegistryError};

use super::contexts::{DestroyCtx, MemPressureCtx, NodeResourceInitContext, TickContext};
use super::node_error::NodeError;
use super::{ControlNode, RenderNode};
use crate::engine::memory_pressure::PressureLevel;

/// Result of a produced-slot request against a runtime node.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ProduceResult {
    Produced,
    Unsupported,
}

/// Runtime node instance for the demand-driven engine spine.
pub trait NodeRuntime {
    /// Allocate [`RuntimeBufferId`] slots owned by this node before first use.
    ///
    /// Default: no-op. [`crate::engine::Engine::attach_runtime_node`] invokes this immediately
    /// before storing the alive node.
    fn init_resources(&mut self, _ctx: &mut NodeResourceInitContext<'_>) -> Result<(), NodeError> {
        Ok(())
    }

    /// Materialize a produced slot.
    ///
    /// Value-producing nodes should update the runtime state backing `slot`.
    /// Nodes with no produced values may keep the default unsupported result.
    fn produce(
        &mut self,
        _slot: &SlotPath,
        _ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        Ok(ProduceResult::Unsupported)
    }

    /// Consume graph inputs as an every-frame demand root.
    ///
    /// Output-like boundary nodes use this for side effects. Nodes that only
    /// produce values can keep the no-op default.
    fn consume(&mut self, _ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        Ok(())
    }

    fn destroy(&mut self, ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError>;

    fn handle_memory_pressure(
        &mut self,
        level: PressureLevel,
        ctx: &mut MemPressureCtx<'_>,
    ) -> Result<(), NodeError>;

    /// Node-owned runtime state exposed as a slot root.
    ///
    /// Nodes without public runtime state return `None`; they do not publish a
    /// synthetic state root in project-read snapshots.
    fn runtime_state_slots(&self) -> Option<&dyn SlotAccess> {
        None
    }

    /// Register any shape roots required by [`Self::runtime_state_slots`].
    fn register_runtime_state_shapes(
        &self,
        _registry: &mut SlotShapeRegistry,
    ) -> Result<(), SlotShapeRegistryError> {
        Ok(())
    }

    /// Sink buffer backing an [`crate::nodes::OutputNode`] after [`Self::init_resources`] runs.
    fn runtime_output_sink_buffer_id(&self) -> Option<RuntimeBufferId> {
        None
    }

    /// Render capability for nodes whose produced slots can materialize visual products.
    fn render_node(&mut self) -> Option<&mut dyn RenderNode> {
        None
    }

    /// Control capability for nodes whose produced slots can render device-control samples.
    fn control_node(&mut self) -> Option<&mut dyn ControlNode> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;

    use crate::dataflow::resolver::{
        ResolveHost, ResolveSession, ResolveTrace, Resolver, SessionHostResolver, TickResolver,
        resolve_trace::ResolveLogLevel,
    };
    use lpc_model::{NodeId, Revision, SlotShapeRegistry};

    struct EmptyResolveHost;

    impl ResolveHost for EmptyResolveHost {
        fn produce(
            &mut self,
            _query: &crate::dataflow::resolver::QueryKey,
            _session: &mut ResolveSession<'_>,
        ) -> Result<
            crate::dataflow::resolver::Production,
            crate::dataflow::resolver::SessionResolveError,
        > {
            Err(crate::dataflow::resolver::SessionResolveError::other(
                "EmptyResolveHost: unexpected produce",
            ))
        }
    }

    struct DummyNode;

    impl DummyNode {
        fn new() -> Self {
            Self
        }
    }

    impl NodeRuntime for DummyNode {
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

    #[test]
    fn node_trait_is_object_safe() {
        let node: Box<dyn NodeRuntime> = Box::new(DummyNode::new());
        assert!(core::mem::size_of_val(&node) > 0);
    }

    #[test]
    fn default_runtime_state_is_absent() {
        let node = DummyNode::new();
        assert!(node.runtime_state_slots().is_none());

        let mut res = Resolver::new();
        let frame = Revision::new(0);
        let mut session =
            ResolveSession::new(frame, &mut res, ResolveTrace::new(ResolveLogLevel::Off));
        let mut host = EmptyResolveHost;
        let slot_shapes = SlotShapeRegistry::default();

        let mut bridge = SessionHostResolver {
            session: &mut session,
            host: &mut host,
        };
        let mut tick = TickContext::new(
            NodeId::new(0),
            frame,
            &mut bridge as &mut dyn TickResolver,
            &slot_shapes,
        );
        let mut dyn_node: Box<dyn NodeRuntime> = Box::new(DummyNode::new());
        assert_eq!(
            dyn_node
                .produce(&SlotPath::root(), &mut tick)
                .expect("produce"),
            ProduceResult::Unsupported
        );
    }
}
