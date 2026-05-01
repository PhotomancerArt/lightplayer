//! Engine spine [`Node`] trait: tick, destroy, memory pressure, and produced props.

use crate::prop::RuntimePropAccess;

use super::contexts::{DestroyCtx, MemPressureCtx, TickContext};
use super::node_error::NodeError;
use super::pressure_level::PressureLevel;

/// Runtime node instance for the new spine (`node/`). Distinct from legacy
/// [`crate::nodes::LegacyNodeRuntime`].
pub trait Node {
    fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError>;

    fn destroy(&mut self, ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError>;

    fn handle_memory_pressure(
        &mut self,
        level: PressureLevel,
        ctx: &mut MemPressureCtx<'_>,
    ) -> Result<(), NodeError>;

    fn props(&self) -> &dyn RuntimePropAccess;
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
        ) -> Result<crate::resolver::ProducedValue, crate::resolver::SessionResolveError> {
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
