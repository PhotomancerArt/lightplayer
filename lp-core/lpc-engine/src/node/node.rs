//! Engine spine [`Node`] trait: tick, destroy, memory pressure, and produced props.

use crate::prop::{
    EMPTY_RUNTIME_OUTPUTS, EMPTY_RUNTIME_STATE, RuntimeOutputAccess, RuntimePropAccess,
    RuntimeStateAccess,
};

use crate::render_product::RenderProductId;
use crate::runtime_buffer::RuntimeBufferId;

use lpc_model::NodeId;

use super::contexts::{DestroyCtx, MemPressureCtx, NodeResourceInitContext, TickContext};
use super::node_error::NodeError;
use super::pressure_level::PressureLevel;

/// M4.1 client sync: identifiers needed to wire [`lpc_wire::legacy::FixtureState`] refs.
#[derive(Clone, Copy, Debug)]
pub struct FixtureProjectionInfo {
    pub lamp_colors_buffer_id: Option<RuntimeBufferId>,
    pub output_sink_buffer_id: RuntimeBufferId,
    pub texture_node_id: NodeId,
}

/// M4.1 shader detail projection (`glsl_code`, error text, semantic render-product ref).
#[derive(Clone, Copy, Debug)]
pub struct ShaderProjectionWire<'a> {
    pub glsl_source: &'a str,
    pub compilation_error: Option<&'a str>,
    pub render_product_id: Option<RenderProductId>,
}

/// Runtime node instance for the new spine (`node/`). Distinct from legacy
/// [`crate::nodes::LegacyNodeRuntime`].
pub trait Node {
    /// Allocate [`RenderProductId`] / [`RuntimeBufferId`] slots owned by this node before first tick.
    ///
    /// Default: no-op. [`crate::engine::Engine::attach_runtime_node`] invokes this immediately
    /// before storing the alive node.
    fn init_resources(&mut self, _ctx: &mut NodeResourceInitContext<'_>) -> Result<(), NodeError> {
        Ok(())
    }

    fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError>;

    fn destroy(&mut self, ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError>;

    fn handle_memory_pressure(
        &mut self,
        level: PressureLevel,
        ctx: &mut MemPressureCtx<'_>,
    ) -> Result<(), NodeError>;

    fn props(&self) -> &dyn RuntimePropAccess;

    /// Node-owned non-scalar products (e.g. render handles). Default: none.
    fn outputs(&self) -> &dyn RuntimeOutputAccess {
        &EMPTY_RUNTIME_OUTPUTS
    }

    /// Reserved for sync/debug state snapshots. Default: empty [`RuntimeStateAccess`].
    fn runtime_state(&self) -> &dyn RuntimeStateAccess {
        &EMPTY_RUNTIME_STATE
    }

    /// Sink buffer backing an [`crate::nodes::OutputNode`] after [`Self::init_resources`] runs.
    fn runtime_output_sink_buffer_id(&self) -> Option<RuntimeBufferId> {
        None
    }

    /// Primary render product allocated for shader output (shader nodes only).
    fn primary_render_product_id(&self) -> Option<RenderProductId> {
        None
    }

    /// Fixture-only: lamp-colors buffer handle, fixture output sink buffer, sampled texture node.
    ///
    /// Default `None`; [`crate::nodes::FixtureNode`] supplies values for legacy detail projection.
    fn fixture_projection_info(&self) -> Option<FixtureProjectionInfo> {
        None
    }

    /// Shader-only: textual source/error plus optional initialized render-product id for wire sync.
    fn shader_projection_wire(&self) -> Option<ShaderProjectionWire<'_>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use alloc::vec::Vec;

    use crate::artifact::ArtifactId;
    use crate::resolver::{
        ResolveHost, ResolveSession, ResolveTrace, Resolver, SessionHostResolver, TickResolver,
        resolve_trace::ResolveLogLevel,
    };
    use lpc_model::prop::prop_path::parse_path;
    use lpc_model::{FrameId, NodeId, PropPath};
    use lps_shared::LpsValueF32;

    struct EmptyResolveHost;

    impl ResolveHost for EmptyResolveHost {
        fn produce(
            &mut self,
            _query: &crate::resolver::QueryKey,
            _session: &mut ResolveSession<'_>,
        ) -> Result<crate::resolver::Production, crate::resolver::SessionResolveError> {
            Err(crate::resolver::SessionResolveError::other(
                "EmptyResolveHost: unexpected produce",
            ))
        }
    }

    struct DummyProps {
        values: Vec<(PropPath, LpsValueF32, FrameId)>,
    }

    impl Default for DummyProps {
        fn default() -> Self {
            Self { values: Vec::new() }
        }
    }

    impl RuntimePropAccess for DummyProps {
        fn get(&self, path: &PropPath) -> Option<(LpsValueF32, FrameId)> {
            self.values
                .iter()
                .find(|(p, _, _)| p == path)
                .map(|(_, v, f)| (v.clone(), *f))
        }

        fn iter_changed_since<'a>(
            &'a self,
            since: FrameId,
        ) -> Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'a> {
            Box::new(
                self.values
                    .iter()
                    .filter(move |(_, _, frame)| frame.as_i64() > since.as_i64())
                    .map(|(p, v, f)| (p.clone(), v.clone(), *f)),
            )
        }

        fn snapshot<'a>(
            &'a self,
        ) -> Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'a> {
            Box::new(
                self.values
                    .iter()
                    .map(|(p, v, f)| (p.clone(), v.clone(), *f)),
            )
        }
    }

    struct DummyNode {
        props: DummyProps,
    }

    impl DummyNode {
        fn new() -> Self {
            let mut props = DummyProps::default();
            let path = parse_path("out").expect("path");
            props
                .values
                .push((path, LpsValueF32::F32(0.25), FrameId::new(1)));
            Self { props }
        }
    }

    impl Node for DummyNode {
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

    #[test]
    fn node_trait_is_object_safe() {
        let node: Box<dyn Node> = Box::new(DummyNode::new());
        assert!(core::mem::size_of_val(&node) > 0);
    }

    #[test]
    fn props_returns_runtime_prop_access() {
        let node = DummyNode::new();
        let path = parse_path("out").expect("path");
        let got = node.props().get(&path);
        assert!(got.is_some());
        assert!(got.unwrap().0.eq(&LpsValueF32::F32(0.25)));

        let registry = crate::binding::BindingRegistry::new();
        let mut res = Resolver::new();
        let frame = FrameId::new(0);
        let mut session = ResolveSession::new(
            frame,
            &mut res,
            &registry,
            ResolveTrace::new(ResolveLogLevel::Off),
        );
        let mut host = EmptyResolveHost;

        let mut bridge = SessionHostResolver {
            session: &mut session,
            host: &mut host,
        };
        let mut tick = TickContext::new(
            NodeId::new(0),
            frame,
            ArtifactId::from_raw(1),
            FrameId::new(0),
            &mut bridge as &mut dyn TickResolver,
        );
        let mut dyn_node: Box<dyn Node> = Box::new(DummyNode::new());
        dyn_node.tick(&mut tick).expect("tick");

        let from_dyn = dyn_node.props().get(&path);
        assert!(from_dyn.is_some());
        assert!(from_dyn.unwrap().0.eq(&LpsValueF32::F32(0.25)));
    }
}
