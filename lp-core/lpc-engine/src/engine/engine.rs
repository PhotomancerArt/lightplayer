//! [`Engine`] — owns spine state and mediates [`ResolveHost`] production for node outputs.

use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::prop::prop_path::{PropPath, Segment};
use lpc_model::{FrameId, NodeId, TreePath, Versioned};

use crate::artifact::ArtifactManager;
use crate::binding::{BindingRegistry, BindingTarget};
use crate::node::{Node, TickContext};
use crate::resolver::{
    ProducedValue, ProductionSource, QueryKey, ResolveHost, ResolveLogLevel, ResolveSession,
    ResolveTrace, Resolver, SessionHostResolver, SessionResolveError, TickResolver,
};
use crate::runtime::frame_time::FrameTime;
use crate::tree::{EntryState, NodeTree};

use super::EngineError;

/// Conventional demand input used by the M2 engine slice.
pub(super) fn default_demand_input_path() -> PropPath {
    let mut path = Vec::new();
    path.push(Segment::Field(String::from("in")));
    path
}

/// Core runtime owner for the demand-driven spine (M2).
pub struct Engine {
    frame_id: FrameId,
    frame_time: FrameTime,
    tree: NodeTree<Box<dyn Node>>,
    bindings: BindingRegistry,
    resolver: Resolver,
    artifacts: ArtifactManager<()>,
    demand_roots: Vec<NodeId>,
}

impl Engine {
    pub fn new(root_path: TreePath) -> Self {
        let frame = FrameId::default();
        Self {
            frame_id: frame,
            frame_time: FrameTime::zero(),
            tree: NodeTree::new(root_path, frame),
            bindings: BindingRegistry::new(),
            resolver: Resolver::new(),
            artifacts: ArtifactManager::new(),
            demand_roots: Vec::new(),
        }
    }

    pub fn frame_id(&self) -> FrameId {
        self.frame_id
    }

    pub fn frame_time(&self) -> FrameTime {
        self.frame_time
    }

    pub fn tree(&self) -> &NodeTree<Box<dyn Node>> {
        &self.tree
    }

    pub fn tree_mut(&mut self) -> &mut NodeTree<Box<dyn Node>> {
        &mut self.tree
    }

    pub fn bindings(&self) -> &BindingRegistry {
        &self.bindings
    }

    pub fn bindings_mut(&mut self) -> &mut BindingRegistry {
        &mut self.bindings
    }

    pub fn resolver(&self) -> &Resolver {
        &self.resolver
    }

    pub fn resolver_mut(&mut self) -> &mut Resolver {
        &mut self.resolver
    }

    pub fn artifacts(&self) -> &ArtifactManager<()> {
        &self.artifacts
    }

    pub fn artifacts_mut(&mut self) -> &mut ArtifactManager<()> {
        &mut self.artifacts
    }

    pub fn demand_roots(&self) -> &[NodeId] {
        &self.demand_roots
    }

    pub fn add_demand_root(&mut self, node: NodeId) {
        self.demand_roots.push(node);
    }

    /// Attach a runtime [`Node`] to an existing tree entry (typically `Pending`).
    pub fn attach_runtime_node(
        &mut self,
        id: NodeId,
        runtime: Box<dyn Node>,
        frame: FrameId,
    ) -> Result<(), EngineError> {
        let entry = self.tree.get_mut(id).ok_or(EngineError::UnknownNode(id))?;
        entry.set_state(EntryState::Alive(runtime), frame);
        Ok(())
    }

    pub fn tick(&mut self, delta_ms: u32) -> Result<(), EngineError> {
        self.resolver.clear_frame_cache();
        self.frame_id = self.frame_id.next();
        self.frame_time =
            FrameTime::new(delta_ms, self.frame_time.total_ms.saturating_add(delta_ms));

        let demand_input = default_demand_input_path();
        let tick_after_resolve: Vec<bool> = self
            .demand_roots
            .iter()
            .map(|&root| self.node_input_is_bound(root, &demand_input))
            .collect();

        let mut resolver = core::mem::replace(&mut self.resolver, Resolver::new());
        let trace = ResolveTrace::new(ResolveLogLevel::Off);
        let mut session = ResolveSession::new(self.frame_id, &mut resolver, &self.bindings, trace);

        let mut producers_ticked = BTreeSet::new();
        let mut host = EngineResolveHost {
            tree: &mut self.tree,
            artifacts: &self.artifacts,
            producers_ticked: &mut producers_ticked,
        };

        {
            for (i, &root) in self.demand_roots.iter().enumerate() {
                session
                    .resolve(
                        &mut host,
                        QueryKey::NodeInput {
                            node: root,
                            input: demand_input.clone(),
                        },
                    )
                    .map_err(EngineError::from)?;

                if tick_after_resolve[i] {
                    tick_tree_node(&mut session, &mut host, root)?;
                }
            }
        }

        self.resolver = resolver;
        Ok(())
    }

    fn node_input_is_bound(&self, node: NodeId, input: &PropPath) -> bool {
        self.bindings.iter().any(|e| {
            matches!(
                &e.target,
                BindingTarget::NodeInput {
                    node: n,
                    input: p,
                } if *n == node && p == input
            )
        })
    }
}

/// Host adapter with borrows disjoint from the [`Resolver`] handed to [`ResolveSession`].
struct EngineResolveHost<'a> {
    tree: &'a mut NodeTree<Box<dyn Node>>,
    artifacts: &'a ArtifactManager<()>,
    producers_ticked: &'a mut BTreeSet<NodeId>,
}

impl EngineResolveHost<'_> {
    fn tick_node_once_for_output(
        &mut self,
        node_id: NodeId,
        session: &mut ResolveSession<'_>,
    ) -> Result<(), SessionResolveError> {
        if self.producers_ticked.contains(&node_id) {
            return Ok(());
        }

        let frame = session.frame_id();
        let restore_frame = session.frame_id();
        let (artifact_id, content_frame, mut node_runtime) = {
            let entry = self.tree.get_mut(node_id).ok_or_else(|| {
                SessionResolveError::other(format!("produce: unknown node {node_id:?}"))
            })?;
            let artifact_id = entry.artifact;
            let content_frame = self
                .artifacts
                .content_frame(&artifact_id)
                .unwrap_or_default();

            let stolen = core::mem::replace(&mut entry.state, EntryState::Pending);
            let node_runtime = match stolen {
                EntryState::Alive(n) => n,
                other => {
                    entry.state = other;
                    return Err(SessionResolveError::other(format!(
                        "produce: node {node_id:?} not alive"
                    )));
                }
            };
            (artifact_id, content_frame, node_runtime)
        };

        let tick_result = {
            let mut bridge = SessionHostResolver {
                session,
                host: self as &mut dyn ResolveHost,
            };
            let resolver_dyn: &mut dyn TickResolver = &mut bridge;
            let mut tick_ctx =
                TickContext::new(node_id, frame, artifact_id, content_frame, resolver_dyn);
            node_runtime.tick(&mut tick_ctx)
        };

        let entry = self.tree.get_mut(node_id).ok_or_else(|| {
            SessionResolveError::other(format!("produce: unknown node {node_id:?}"))
        })?;
        entry.set_state(EntryState::Alive(node_runtime), restore_frame);

        match tick_result {
            Ok(()) => {
                self.producers_ticked.insert(node_id);
                Ok(())
            }
            Err(e) => Err(SessionResolveError::other(format!(
                "produce: tick failed: {e:?}"
            ))),
        }
    }
}

impl ResolveHost for EngineResolveHost<'_> {
    fn produce(
        &mut self,
        query: &QueryKey,
        session: &mut ResolveSession<'_>,
    ) -> Result<ProducedValue, SessionResolveError> {
        match query {
            QueryKey::NodeOutput { node, output } => {
                self.tick_node_once_for_output(*node, session)?;
                let entry = self.tree.get(*node).ok_or_else(|| {
                    SessionResolveError::other(format!("read output: unknown node {node:?}"))
                })?;
                let n = match &entry.state {
                    EntryState::Alive(n) => n,
                    _ => {
                        return Err(SessionResolveError::other(format!(
                            "read output: node {node:?} not alive"
                        )));
                    }
                };
                match n.props().get(output) {
                    Some((v, frame)) => Ok(ProducedValue::new(
                        Versioned::new(frame, v),
                        ProductionSource::NodeOutput {
                            node: *node,
                            output: output.clone(),
                        },
                    )),
                    None => Err(SessionResolveError::other(format!(
                        "missing node output {output:?} on {node:?}"
                    ))),
                }
            }
            QueryKey::NodeInput { node, input } => {
                self.tick_node_once_for_output(*node, session)?;
                let entry = self.tree.get(*node).ok_or_else(|| {
                    SessionResolveError::UnresolvedNodeInput {
                        node: *node,
                        input: input.clone(),
                    }
                })?;
                let n = match &entry.state {
                    EntryState::Alive(n) => n,
                    _ => {
                        return Err(SessionResolveError::UnresolvedNodeInput {
                            node: *node,
                            input: input.clone(),
                        });
                    }
                };
                match n.props().get(input) {
                    Some((v, frame)) => Ok(ProducedValue::new(
                        Versioned::new(frame, v),
                        ProductionSource::Default,
                    )),
                    None => Err(SessionResolveError::UnresolvedNodeInput {
                        node: *node,
                        input: input.clone(),
                    }),
                }
            }
            QueryKey::Bus(_) => Err(SessionResolveError::other(
                "engine host cannot satisfy bus query",
            )),
        }
    }
}

fn tick_tree_node(
    session: &mut ResolveSession<'_>,
    host: &mut EngineResolveHost<'_>,
    node_id: NodeId,
) -> Result<(), EngineError> {
    let frame = session.frame_id();
    let restore_frame = session.frame_id();
    let (artifact_id, content_frame, mut node_runtime) = {
        let entry = host
            .tree
            .get_mut(node_id)
            .ok_or(EngineError::UnknownNode(node_id))?;
        let artifact_id = entry.artifact;
        let content_frame = host
            .artifacts
            .content_frame(&artifact_id)
            .unwrap_or_default();

        let stolen = core::mem::replace(&mut entry.state, EntryState::Pending);
        let node_runtime = match stolen {
            EntryState::Alive(n) => n,
            other => {
                entry.state = other;
                return Err(EngineError::NotAlive(node_id));
            }
        };
        (artifact_id, content_frame, node_runtime)
    };

    let tick_result = {
        let mut bridge = SessionHostResolver {
            session,
            host: host as &mut dyn ResolveHost,
        };
        let resolver_dyn: &mut dyn TickResolver = &mut bridge;
        let mut tick_ctx =
            TickContext::new(node_id, frame, artifact_id, content_frame, resolver_dyn);
        node_runtime.tick(&mut tick_ctx)
    };

    let entry = host
        .tree
        .get_mut(node_id)
        .ok_or(EngineError::UnknownNode(node_id))?;
    entry.set_state(EntryState::Alive(node_runtime), restore_frame);

    tick_result.map_err(|e| EngineError::node(node_id, e))
}

#[cfg(test)]
pub(super) fn resolve_with_engine_host(
    eng: &mut Engine,
    key: QueryKey,
    log_level: ResolveLogLevel,
) -> Result<(ProducedValue, ResolveTrace), SessionResolveError> {
    let fid = eng.frame_id;
    let mut resolver_tmp = core::mem::replace(&mut eng.resolver, Resolver::new());
    resolver_tmp.clear_frame_cache();
    let mut session = ResolveSession::new(
        fid,
        &mut resolver_tmp,
        &eng.bindings,
        ResolveTrace::new(log_level),
    );
    let mut producers_ticked = BTreeSet::new();
    let mut host = EngineResolveHost {
        tree: &mut eng.tree,
        artifacts: &eng.artifacts,
        producers_ticked: &mut producers_ticked,
    };
    let result = session
        .resolve(&mut host, key)
        .map(|pv| (pv, session.trace().clone()));
    eng.resolver = resolver_tmp;
    result
}

#[cfg(test)]
pub(super) fn resolve_twice_same_frame_with_engine_host(
    eng: &mut Engine,
    key: QueryKey,
) -> Result<(ProducedValue, ProducedValue), SessionResolveError> {
    let fid = eng.frame_id;
    let mut resolver_tmp = core::mem::replace(&mut eng.resolver, Resolver::new());
    resolver_tmp.clear_frame_cache();
    let mut session = ResolveSession::new(
        fid,
        &mut resolver_tmp,
        &eng.bindings,
        ResolveTrace::new(ResolveLogLevel::Off),
    );
    let mut producers_ticked = BTreeSet::new();
    let mut host = EngineResolveHost {
        tree: &mut eng.tree,
        artifacts: &eng.artifacts,
        producers_ticked: &mut producers_ticked,
    };
    let result = session.resolve(&mut host, key.clone()).and_then(|first| {
        session
            .resolve(&mut host, key)
            .map(|second| (first, second))
    });
    eng.resolver = resolver_tmp;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use lps_shared::LpsValueF32;

    use crate::binding::BindingError;
    use crate::engine::test_support::{
        EngineTestBuilder, bus, literal, node_output, output, path, trace_has_value_origin_path,
    };

    #[test]
    fn engine_new_has_frame_state_and_empty_registry_resolver_tree_root() {
        let eng = Engine::new(TreePath::parse("/show.t").expect("path"));
        assert_eq!(eng.frame_id(), FrameId::default());
        assert_eq!(eng.frame_time(), FrameTime::zero());
        assert!(eng.bindings().iter().next().is_none());
        assert!(eng.resolver().cache().is_empty());
        assert_eq!(eng.tree().len(), 1);
    }

    #[test]
    fn tick_advances_frame_id_and_accumulates_frame_time() {
        let mut eng = Engine::new(TreePath::parse("/show.t").expect("path"));
        eng.tick(10).expect("tick");
        assert_eq!(eng.frame_id().as_i64(), 1);
        assert_eq!(eng.frame_time().delta_ms, 10);
        assert_eq!(eng.frame_time().total_ms, 10);
        eng.tick(5).expect("tick");
        assert_eq!(eng.frame_id().as_i64(), 2);
        assert_eq!(eng.frame_time().total_ms, 15);
    }

    #[test]
    fn fixture_resolves_shader_output_through_bus() {
        let mut h = EngineTestBuilder::new()
            .shader("shader", output("outputs[0]", 0.75))
            .fixture("fixture")
            .output_node("output")
            .bind_bus("video_out", node_output("shader", "outputs[0]"))
            .bind_demand_input("fixture", bus("video_out"))
            .bind_demand_input("output", bus("video_out"))
            .demand_root("fixture")
            .demand_root("output")
            .build();

        h.engine.tick(1).expect("tick");

        assert_eq!(h.fixture_f32("fixture"), Some(0.75));
        assert_eq!(h.output_f32("output"), Some(0.75));
        assert_eq!(h.shader_ticks("shader"), 1);
    }

    #[test]
    fn demand_roots_resolve_inside_resolve_session_while_session_is_live() {
        let mut h = EngineTestBuilder::new()
            .shader("shader", output("outputs[0]", 2.0))
            .fixture("fixture")
            .bind_bus("video", node_output("shader", "outputs[0]"))
            .bind_demand_input("fixture", bus("video"))
            .demand_root("fixture")
            .build();
        h.engine.tick(1).expect("tick");
        assert!(
            !h.engine.resolver().cache().is_empty(),
            "resolver cache should hold demand-driven values after tick"
        );
    }

    #[test]
    fn node_output_host_reads_runtime_prop_access() {
        let mut h = EngineTestBuilder::new()
            .shader("shader", output("outputs[0]", 2.0))
            .build();

        let out = path("outputs[0]");
        let shader = h.node("shader");
        let a = h
            .resolve(QueryKey::NodeOutput {
                node: shader,
                output: out,
            })
            .expect("resolve");
        assert!(a.value.get().eq(&LpsValueF32::F32(2.0)));
    }

    #[test]
    fn producer_runs_once_when_demanded_twice_in_same_frame() {
        let mut h = EngineTestBuilder::new()
            .shader("shader", output("outputs[0]", 2.0))
            .build();
        h.reset_shader_ticks("shader");
        let out = path("outputs[0]");
        let key = QueryKey::NodeOutput {
            node: h.node("shader"),
            output: out,
        };

        super::resolve_twice_same_frame_with_engine_host(&mut h.engine, key).expect("resolve pair");

        assert_eq!(h.shader_ticks("shader"), 1);
    }

    #[test]
    fn bus_selects_highest_priority_binding() {
        let mut h = EngineTestBuilder::new()
            .bind_bus_with_priority("video", literal(0.25), 1)
            .expect("low priority")
            .bind_bus_with_priority("video", literal(0.9), 10)
            .expect("high priority")
            .build();

        let pv = h.resolve_bus("video").expect("resolve bus");

        assert!(pv.value.get().eq(&LpsValueF32::F32(0.9)));
    }

    #[test]
    fn equal_priority_bus_bindings_error() {
        let builder = EngineTestBuilder::new()
            .bind_bus_with_priority("video", literal(0.25), 7)
            .expect("first binding");

        let err = match builder.bind_bus_with_priority("video", literal(0.9), 7) {
            Ok(_) => panic!("equal priority bus providers should fail"),
            Err(e) => e,
        };

        assert!(matches!(
            err,
            BindingError::DuplicateProviderPriority { .. }
        ));
    }

    #[test]
    fn recursive_bus_cycle_errors() {
        let mut h = EngineTestBuilder::new()
            .bind_bus("a", bus("b"))
            .bind_bus("b", bus("a"))
            .build();

        let err = h.resolve_bus("a").expect_err("cycle");

        assert!(matches!(err, SessionResolveError::Cycle { .. }));
    }

    #[test]
    fn resolve_trace_records_value_origin_path() {
        let mut h = EngineTestBuilder::new()
            .shader("shader", output("outputs[0]", 0.5))
            .bind_bus("video", node_output("shader", "outputs[0]"))
            .build();
        let out = path("outputs[0]");

        let (_, trace) = h
            .resolve_with_trace(QueryKey::Bus(lpc_model::ChannelName(String::from("video"))))
            .expect("resolve with trace");

        assert!(trace_has_value_origin_path(
            &trace,
            "video",
            h.node("shader"),
            &out,
        ));
    }

    #[test]
    fn binding_registry_versions_are_available_for_debug_list() {
        let h = EngineTestBuilder::new()
            .shader("shader", output("outputs[0]", 0.5))
            .fixture("fixture")
            .bind_bus("video", node_output("shader", "outputs[0]"))
            .bind_demand_input("fixture", bus("video"))
            .build();
        let versions: Vec<_> = h.engine.bindings().iter().map(|e| e.version).collect();

        assert_eq!(versions, alloc::vec![FrameId::new(1), FrameId::new(1)]);
    }
}
