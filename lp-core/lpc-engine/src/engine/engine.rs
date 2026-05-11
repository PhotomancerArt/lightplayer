//! [`Engine`] — owns spine state and mediates [`ResolveHost`] production for produced slots.

use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::format;
use alloc::sync::Arc;
use alloc::vec::Vec;

use lpc_model::{
    LpValue, NodeId, Revision, SlotAccessor, SlotDataAccess, SlotPath, SlotShapeRegistry, TreePath,
    WithRevision, advance_revision, current_revision, lookup_slot_data,
};

use crate::artifact::{ArtifactState, ArtifactStore};
use crate::binding::{BindingDraft, BindingError, BindingRef};
use crate::gfx::LpGraphics;
use crate::node::{
    NodeCall, NodeCallKey, NodeResourceInitContext, NodeRuntime, RenderContext, TickContext,
};
use crate::node::{NodeEntryState, NodeTree};
use crate::render_product::{RenderProduct, RenderTextureRequest, TextureRenderProduct};
use crate::resolver::resolver::model_value_to_lps_value_f32;
use crate::resolver::{
    EngineSession, Production, ProductionSource, QueryKey, ResolveHost, ResolveLogLevel,
    ResolveTrace, Resolver, SessionHostResolver, SessionResolveError, TickResolver,
};
use crate::runtime::frame_num::FrameNum;
use crate::runtime::frame_time::FrameTime;
use crate::runtime_buffer::{RuntimeBufferId, RuntimeBufferStore};
use crate::runtime_product::RuntimeProduct;

use super::EngineError;

/// Conventional demand input used by the M2 engine slice.
pub(crate) fn default_demand_input_path() -> SlotPath {
    SlotPath::parse("in").expect("default demand input slot path")
}

fn runtime_product_from_lp_value(value: LpValue) -> Result<RuntimeProduct, SessionResolveError> {
    match value {
        LpValue::RenderProduct(product) => Ok(RuntimeProduct::render(product)),
        other => model_value_to_lps_value_f32(&other)
            .map(RuntimeProduct::Value)
            .map_err(|e| SessionResolveError::other(e.message)),
    }
}

/// Core runtime owner for the demand-driven spine (M2).
pub struct Engine {
    frame_num: FrameNum,
    revision: Revision,
    frame_time: FrameTime,
    tree: NodeTree<Box<dyn NodeRuntime>>,
    resolver: Resolver,
    slot_shapes: SlotShapeRegistry,
    runtime_buffers: RuntimeBufferStore,
    artifacts: ArtifactStore,
    demand_roots: Vec<NodeId>,
    graphics: Option<Arc<dyn LpGraphics>>,
}

impl Engine {
    pub fn new(root_path: TreePath) -> Self {
        let revision = Revision::default();
        let mut slot_shapes = SlotShapeRegistry::default();
        lpc_model::slot_shapes::register_all_static_slot_shapes(&mut slot_shapes)
            .expect("static slot shapes register without conflicts");
        Self {
            frame_num: FrameNum::default(),
            revision,
            frame_time: FrameTime::zero(),
            tree: NodeTree::new(root_path, revision),
            resolver: Resolver::new(),
            slot_shapes,
            runtime_buffers: RuntimeBufferStore::new(),
            artifacts: ArtifactStore::new(),
            demand_roots: Vec::new(),
            graphics: None,
        }
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn frame_num(&self) -> FrameNum {
        self.frame_num
    }

    pub fn frame_time(&self) -> FrameTime {
        self.frame_time
    }

    pub fn tree(&self) -> &NodeTree<Box<dyn NodeRuntime>> {
        &self.tree
    }

    pub fn tree_mut(&mut self) -> &mut NodeTree<Box<dyn NodeRuntime>> {
        &mut self.tree
    }

    pub fn resolver(&self) -> &Resolver {
        &self.resolver
    }

    pub fn resolver_mut(&mut self) -> &mut Resolver {
        &mut self.resolver
    }

    pub fn slot_shapes(&self) -> &SlotShapeRegistry {
        &self.slot_shapes
    }

    pub fn slot_shapes_mut(&mut self) -> &mut SlotShapeRegistry {
        &mut self.slot_shapes
    }

    pub fn runtime_buffers(&self) -> &RuntimeBufferStore {
        &self.runtime_buffers
    }

    pub fn runtime_buffers_mut(&mut self) -> &mut RuntimeBufferStore {
        &mut self.runtime_buffers
    }

    pub fn artifacts(&self) -> &ArtifactStore {
        &self.artifacts
    }

    pub fn artifacts_mut(&mut self) -> &mut ArtifactStore {
        &mut self.artifacts
    }

    pub fn demand_roots(&self) -> &[NodeId] {
        &self.demand_roots
    }

    pub fn add_demand_root(&mut self, node: NodeId) {
        self.demand_roots.push(node);
    }

    pub fn add_binding(
        &mut self,
        draft: BindingDraft,
        revision: Revision,
    ) -> Result<BindingRef, BindingError> {
        self.tree.add_binding(draft, revision)
    }

    /// Optional graphics backend for core shader nodes; clone is cheap (`Arc`).
    pub fn set_graphics(&mut self, graphics: Option<Arc<dyn LpGraphics>>) {
        self.graphics = graphics;
    }

    pub fn graphics(&self) -> Option<&Arc<dyn LpGraphics>> {
        self.graphics.as_ref()
    }

    /// Attach a runtime [`NodeRuntime`] to an existing tree entry (typically `Pending`).
    ///
    /// Runs [`NodeRuntime::init_resources`] on `runtime` first so nodes can allocate store-backed ids before
    /// becoming [`NodeEntryState::Alive`].
    pub fn attach_runtime_node(
        &mut self,
        id: NodeId,
        mut runtime: Box<dyn NodeRuntime>,
        frame: Revision,
    ) -> Result<(), EngineError> {
        let mut ctx = NodeResourceInitContext::new(&mut self.runtime_buffers);
        runtime
            .init_resources(&mut ctx)
            .map_err(|e| EngineError::node(id, e))?;
        runtime
            .register_runtime_state_shapes(&mut self.slot_shapes)
            .map_err(|e| EngineError::Node {
                node: id,
                message: format!("runtime state shape registration: {e}"),
            })?;
        let entry = self.tree.get_mut(id).ok_or(EngineError::UnknownNode(id))?;
        entry.set_state(NodeEntryState::Alive(runtime), frame);
        Ok(())
    }

    pub fn runtime_output_sink_buffer_id(&self, node_id: NodeId) -> Option<RuntimeBufferId> {
        let entry = self.tree.get(node_id)?;
        match entry.state.value() {
            NodeEntryState::Alive(node) => node.runtime_output_sink_buffer_id(),
            _ => None,
        }
    }

    pub fn tick(&mut self, delta_ms: u32) -> Result<(), EngineError> {
        self.resolver.clear_frame_cache();
        self.frame_num = self.frame_num.next();
        self.revision = advance_revision();
        self.frame_time =
            FrameTime::new(delta_ms, self.frame_time.total_ms.saturating_add(delta_ms));

        let demand_input = default_demand_input_path();
        let tick_after_resolve: Vec<bool> = self
            .demand_roots
            .iter()
            .map(|&root| self.consumed_slot_is_bound(root, &demand_input))
            .collect();

        let mut resolver = core::mem::replace(&mut self.resolver, Resolver::new());
        let trace = ResolveTrace::new(ResolveLogLevel::Off);
        let mut session = EngineSession::new(self.revision, &mut resolver, trace);

        let mut producers_ticked = BTreeSet::new();
        let time_s = self.frame_time.total_ms as f32 / 1000.0;
        let mut host = EngineResolveHost {
            tree: &mut self.tree,
            artifacts: &self.artifacts,
            producers_ticked: &mut producers_ticked,
            runtime_buffers: &mut self.runtime_buffers,
            slot_shapes: &self.slot_shapes,
            graphics: self.graphics.clone(),
            frame_time_seconds: time_s,
        };

        {
            for (i, &root) in self.demand_roots.iter().enumerate() {
                session
                    .resolve(
                        &mut host,
                        QueryKey::ConsumedSlot {
                            node: root,
                            slot: demand_input.clone(),
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

    #[cfg(test)]
    pub(crate) fn render_texture_for_test(
        &mut self,
        product: RenderProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, SessionResolveError> {
        let mut producers_ticked = BTreeSet::new();
        let time_s = self.frame_time.total_ms as f32 / 1000.0;
        let mut host = EngineResolveHost {
            tree: &mut self.tree,
            artifacts: &self.artifacts,
            producers_ticked: &mut producers_ticked,
            runtime_buffers: &mut self.runtime_buffers,
            slot_shapes: &self.slot_shapes,
            graphics: self.graphics.clone(),
            frame_time_seconds: time_s,
        };
        host.render_node_texture(product, request)
    }

    fn consumed_slot_is_bound(&self, node: NodeId, slot: &SlotPath) -> bool {
        self.tree.binding_for_consumed_slot(node, slot).is_some()
    }
}

/// Host adapter with borrows disjoint from the [`Resolver`] handed to [`EngineSession`].
struct EngineResolveHost<'a> {
    tree: &'a mut NodeTree<Box<dyn NodeRuntime>>,
    artifacts: &'a ArtifactStore,
    producers_ticked: &'a mut BTreeSet<NodeId>,
    runtime_buffers: &'a mut RuntimeBufferStore,
    slot_shapes: &'a SlotShapeRegistry,
    graphics: Option<Arc<dyn LpGraphics>>,
    frame_time_seconds: f32,
}

impl EngineResolveHost<'_> {
    fn tick_node_once_for_output(
        &mut self,
        node_id: NodeId,
        session: &mut EngineSession<'_>,
    ) -> Result<(), SessionResolveError> {
        if self.producers_ticked.contains(&node_id) {
            return Ok(());
        }

        let revision = session.revision();
        let restore_frame = session.revision();
        let (artifact_id, content_frame, mut node_runtime) = {
            let entry = self.tree.get_mut(node_id).ok_or_else(|| {
                SessionResolveError::other(format!("produce: unknown node {node_id:?}"))
            })?;
            let artifact_id = entry.artifact();
            let content_frame = self
                .artifacts
                .content_frame(&artifact_id)
                .unwrap_or_default();

            let old_changed_at = entry.state.changed_at();
            let executing = NodeEntryState::Executing {
                call: NodeCallKey::new(node_id, NodeCall::Tick),
            };
            let stolen = core::mem::replace(
                &mut entry.state,
                WithRevision::new(old_changed_at, executing),
            );
            let node_runtime = match stolen.into_value() {
                NodeEntryState::Alive(n) => n,
                NodeEntryState::Executing { call } => {
                    entry.state = WithRevision::new(
                        old_changed_at,
                        NodeEntryState::Executing { call: call.clone() },
                    );
                    return Err(SessionResolveError::other(format!(
                        "node {node_id:?} is already executing {}; re-entry through EngineSession is unsupported",
                        call.call.label()
                    )));
                }
                other => {
                    entry.state = WithRevision::new(old_changed_at, other);
                    return Err(SessionResolveError::other(format!(
                        "produce: node {node_id:?} not alive"
                    )));
                }
            };
            (artifact_id, content_frame, node_runtime)
        };

        let gfx = self.graphics.clone();
        let time_s = self.frame_time_seconds;
        let slot_shapes = self.slot_shapes;
        let tick_result = {
            let mut bridge = SessionHostResolver {
                session,
                host: self as &mut dyn ResolveHost,
            };
            let resolver_dyn: &mut dyn TickResolver = &mut bridge;
            let mut tick_ctx = TickContext::with_render_services(
                node_id,
                revision,
                artifact_id,
                content_frame,
                resolver_dyn,
                slot_shapes,
                gfx,
                time_s,
            );
            node_runtime.tick(&mut tick_ctx)
        };

        let entry = self.tree.get_mut(node_id).ok_or_else(|| {
            SessionResolveError::other(format!("produce: unknown node {node_id:?}"))
        })?;
        entry.set_state(NodeEntryState::Alive(node_runtime), restore_frame);

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
        session: &mut EngineSession<'_>,
    ) -> Result<Production, SessionResolveError> {
        match query {
            QueryKey::ProducedSlot { node, slot } => {
                self.tick_node_once_for_output(*node, session)?;
                let entry = self.tree.get(*node).ok_or_else(|| {
                    SessionResolveError::other(format!("read output: unknown node {node:?}"))
                })?;
                let n = match entry.state.value() {
                    NodeEntryState::Alive(n) => n,
                    _ => {
                        return Err(SessionResolveError::other(format!(
                            "read output: node {node:?} not alive"
                        )));
                    }
                };
                let product = self.read_runtime_state_product(&**n, slot).map_err(|e| {
                    SessionResolveError::other(format!(
                        "missing produced slot {slot:?} on {node:?}: {e}"
                    ))
                })?;
                Ok(Production::new(
                    product,
                    ProductionSource::ProducedSlot {
                        node: *node,
                        slot: slot.clone(),
                    },
                ))
            }
            QueryKey::ConsumedSlot { node, slot } => {
                let entry = self.tree.get(*node).ok_or_else(|| {
                    SessionResolveError::UnresolvedConsumedSlot {
                        node: *node,
                        slot: slot.clone(),
                    }
                })?;
                let product = self
                    .read_authored_def_product(&entry.def_handle, slot)
                    .map_err(|_| SessionResolveError::UnresolvedConsumedSlot {
                        node: *node,
                        slot: slot.clone(),
                    })?;
                Ok(Production::new(product, ProductionSource::Default))
            }
            QueryKey::ConsumedSlotAccessor { node, accessor } => {
                let entry = self.tree.get(*node).ok_or_else(|| {
                    SessionResolveError::UnresolvedConsumedSlot {
                        node: *node,
                        slot: accessor.path().clone(),
                    }
                })?;
                let product = self
                    .read_authored_def_product_by_accessor(&entry.def_handle, accessor)
                    .map_err(|_| SessionResolveError::UnresolvedConsumedSlot {
                        node: *node,
                        slot: accessor.path().clone(),
                    })?;
                Ok(Production::new(product, ProductionSource::Default))
            }
            QueryKey::Bus(_) => Err(SessionResolveError::other(
                "engine host cannot satisfy bus query",
            )),
        }
    }

    fn binding_for_consumed_slot(
        &self,
        node: NodeId,
        slot: &SlotPath,
    ) -> Option<(BindingRef, crate::binding::BindingEntry)> {
        self.tree
            .binding_for_consumed_slot(node, slot)
            .map(|(binding_ref, entry)| (binding_ref, entry.clone()))
    }

    fn providers_for_bus(
        &self,
        channel: &lpc_model::ChannelName,
    ) -> Vec<(BindingRef, crate::binding::BindingEntry)> {
        self.tree
            .providers_for_bus(channel)
            .into_iter()
            .map(|(binding_ref, entry)| (binding_ref, entry.clone()))
            .collect()
    }

    fn render_texture(
        &mut self,
        product: RenderProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, SessionResolveError> {
        self.render_node_texture(product, request)
    }

    fn runtime_buffer_mut(
        &mut self,
        id: crate::runtime_buffer::RuntimeBufferId,
        frame: Revision,
    ) -> Result<&mut crate::runtime_buffer::RuntimeBuffer, SessionResolveError> {
        self.runtime_buffers
            .get_mut_mark_updated(id, frame)
            .map_err(|e| SessionResolveError::other(format!("runtime buffer mut: {e:?}")))
    }
}

impl EngineResolveHost<'_> {
    fn read_runtime_state_product(
        &self,
        node: &dyn NodeRuntime,
        slot: &SlotPath,
    ) -> Result<WithRevision<RuntimeProduct>, SessionResolveError> {
        let data = lookup_slot_data(node.runtime_state_slots(), self.slot_shapes, slot)
            .map_err(|e| SessionResolveError::other(format!("runtime state lookup: {e}")))?;
        let SlotDataAccess::Value(value) = data else {
            return Err(SessionResolveError::other(format!(
                "runtime state slot {slot:?} is not a value"
            )));
        };
        Ok(WithRevision::new(
            value.changed_at(),
            runtime_product_from_lp_value(value.value())?,
        ))
    }

    fn read_authored_def_product(
        &self,
        handle: &crate::node::NodeDefHandle,
        slot: &SlotPath,
    ) -> Result<WithRevision<RuntimeProduct>, SessionResolveError> {
        if !handle.is_artifact_root() {
            return Err(SessionResolveError::other(format!(
                "non-root node def handles are not supported yet: {}",
                handle.path()
            )));
        }
        let entry = self.artifacts.entry(&handle.artifact()).ok_or_else(|| {
            SessionResolveError::other(format!(
                "node def artifact {:?} is not loaded",
                handle.artifact()
            ))
        })?;
        let def = match &entry.state {
            ArtifactState::Loaded(def)
            | ArtifactState::Prepared(def)
            | ArtifactState::Idle(def) => def,
            other => {
                return Err(SessionResolveError::other(format!(
                    "node def artifact {:?} has no loaded payload: {other:?}",
                    handle.artifact()
                )));
            }
        };
        let data = lookup_slot_data(def, self.slot_shapes, slot)
            .map_err(|e| SessionResolveError::other(format!("authored def lookup: {e}")))?;
        let SlotDataAccess::Value(value) = data else {
            return Err(SessionResolveError::other(format!(
                "authored def slot {slot:?} is not a value"
            )));
        };
        Ok(WithRevision::new(
            value.changed_at(),
            runtime_product_from_lp_value(value.value())?,
        ))
    }

    fn read_authored_def_product_by_accessor(
        &self,
        handle: &crate::node::NodeDefHandle,
        accessor: &SlotAccessor,
    ) -> Result<WithRevision<RuntimeProduct>, SessionResolveError> {
        if !handle.is_artifact_root() {
            return Err(SessionResolveError::other(format!(
                "non-root node def handles are not supported yet: {}",
                handle.path()
            )));
        }
        let entry = self.artifacts.entry(&handle.artifact()).ok_or_else(|| {
            SessionResolveError::other(format!(
                "node def artifact {:?} is not loaded",
                handle.artifact()
            ))
        })?;
        let def = match &entry.state {
            ArtifactState::Loaded(def)
            | ArtifactState::Prepared(def)
            | ArtifactState::Idle(def) => def,
            other => {
                return Err(SessionResolveError::other(format!(
                    "node def artifact {:?} has no loaded payload: {other:?}",
                    handle.artifact()
                )));
            }
        };
        let data = accessor
            .access(def, self.slot_shapes)
            .map_err(|e| SessionResolveError::other(format!("authored def accessor: {e}")))?;
        let SlotDataAccess::Value(value) = data else {
            return Err(SessionResolveError::other(format!(
                "authored def slot {:?} is not a value",
                accessor.path()
            )));
        };
        Ok(WithRevision::new(
            value.changed_at(),
            runtime_product_from_lp_value(value.value())?,
        ))
    }

    fn render_node_texture(
        &mut self,
        product: RenderProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, SessionResolveError> {
        let node_id = product.node();
        let revision = current_revision();
        let mut node_runtime = {
            let entry = self.tree.get_mut(node_id).ok_or_else(|| {
                SessionResolveError::other(format!("render: unknown node {node_id:?}"))
            })?;
            let old_changed_at = entry.state.changed_at();
            let executing = NodeEntryState::Executing {
                call: NodeCallKey::new(node_id, NodeCall::Render { product }),
            };
            let stolen = core::mem::replace(
                &mut entry.state,
                WithRevision::new(old_changed_at, executing),
            );
            match stolen.into_value() {
                NodeEntryState::Alive(n) => n,
                NodeEntryState::Executing { call } => {
                    entry.state = WithRevision::new(
                        old_changed_at,
                        NodeEntryState::Executing { call: call.clone() },
                    );
                    return Err(SessionResolveError::other(format!(
                        "node {node_id:?} is already executing {}; re-entry through EngineSession is unsupported",
                        call.call.label()
                    )));
                }
                other => {
                    entry.state = WithRevision::new(old_changed_at, other);
                    return Err(SessionResolveError::other(format!(
                        "render: node {node_id:?} not alive"
                    )));
                }
            }
        };

        let result = {
            let Some(render_node) = node_runtime.render_node() else {
                return restore_node_after_failed_render(
                    self.tree,
                    node_id,
                    node_runtime,
                    revision,
                    SessionResolveError::other(format!(
                        "node {node_id:?} cannot render product output {}: NodeRuntime::render_node() returned None",
                        product.output()
                    )),
                );
            };
            let mut ctx = RenderContext::new(
                node_id,
                revision,
                self.graphics.clone(),
                self.frame_time_seconds,
            );
            render_node.render_texture(product, request, &mut ctx)
        };

        let entry = self.tree.get_mut(node_id).ok_or_else(|| {
            SessionResolveError::other(format!("render: unknown node {node_id:?}"))
        })?;
        entry.set_state(NodeEntryState::Alive(node_runtime), revision);

        result.map_err(|e| SessionResolveError::other(format!("render: {e:?}")))
    }
}

fn restore_node_after_failed_render(
    tree: &mut NodeTree<Box<dyn NodeRuntime>>,
    node_id: NodeId,
    node_runtime: Box<dyn NodeRuntime>,
    revision: Revision,
    err: SessionResolveError,
) -> Result<TextureRenderProduct, SessionResolveError> {
    if let Some(entry) = tree.get_mut(node_id) {
        entry.set_state(NodeEntryState::Alive(node_runtime), revision);
    }
    Err(err)
}

fn tick_tree_node(
    session: &mut EngineSession<'_>,
    host: &mut EngineResolveHost<'_>,
    node_id: NodeId,
) -> Result<(), EngineError> {
    let revision = session.revision();
    let restore_frame = session.revision();
    let (artifact_id, content_frame, mut node_runtime) = {
        let entry = host
            .tree
            .get_mut(node_id)
            .ok_or(EngineError::UnknownNode(node_id))?;
        let artifact_id = entry.artifact();
        let content_frame = host
            .artifacts
            .content_frame(&artifact_id)
            .unwrap_or_default();

        let old_changed_at = entry.state.changed_at();
        let executing = NodeEntryState::Executing {
            call: NodeCallKey::new(node_id, NodeCall::Tick),
        };
        let stolen = core::mem::replace(
            &mut entry.state,
            WithRevision::new(old_changed_at, executing),
        );
        let node_runtime = match stolen.into_value() {
            NodeEntryState::Alive(n) => n,
            NodeEntryState::Executing { call } => {
                entry.state = WithRevision::new(
                    old_changed_at,
                    NodeEntryState::Executing { call: call.clone() },
                );
                return Err(EngineError::from(SessionResolveError::other(format!(
                    "node {node_id:?} is already executing {}; re-entry through EngineSession is unsupported",
                    call.call.label()
                ))));
            }
            other => {
                entry.state = WithRevision::new(old_changed_at, other);
                return Err(EngineError::NotAlive(node_id));
            }
        };
        (artifact_id, content_frame, node_runtime)
    };

    let gfx = host.graphics.clone();
    let time_s = host.frame_time_seconds;
    let slot_shapes = host.slot_shapes;
    let tick_result = {
        let mut bridge = SessionHostResolver {
            session,
            host: host as &mut dyn ResolveHost,
        };
        let resolver_dyn: &mut dyn TickResolver = &mut bridge;
        let mut tick_ctx = TickContext::with_render_services(
            node_id,
            revision,
            artifact_id,
            content_frame,
            resolver_dyn,
            slot_shapes,
            gfx,
            time_s,
        );
        node_runtime.tick(&mut tick_ctx)
    };

    let entry = host
        .tree
        .get_mut(node_id)
        .ok_or(EngineError::UnknownNode(node_id))?;
    entry.set_state(NodeEntryState::Alive(node_runtime), restore_frame);

    match tick_result {
        Ok(()) => Ok(()),
        Err(e) => Err(EngineError::node(node_id, e)),
    }
}

#[cfg(test)]
pub(crate) fn resolve_with_engine_host(
    eng: &mut Engine,
    key: QueryKey,
    log_level: ResolveLogLevel,
) -> Result<(Production, ResolveTrace), SessionResolveError> {
    let fid = eng.revision;
    let mut resolver_tmp = core::mem::replace(&mut eng.resolver, Resolver::new());
    resolver_tmp.clear_frame_cache();
    let mut session = EngineSession::new(fid, &mut resolver_tmp, ResolveTrace::new(log_level));
    let mut producers_ticked = BTreeSet::new();
    let time_s = eng.frame_time.total_ms as f32 / 1000.0;
    let mut host = EngineResolveHost {
        tree: &mut eng.tree,
        artifacts: &eng.artifacts,
        producers_ticked: &mut producers_ticked,
        runtime_buffers: &mut eng.runtime_buffers,
        slot_shapes: &eng.slot_shapes,
        graphics: eng.graphics.clone(),
        frame_time_seconds: time_s,
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
) -> Result<(Production, Production), SessionResolveError> {
    let fid = eng.revision;
    let mut resolver_tmp = core::mem::replace(&mut eng.resolver, Resolver::new());
    resolver_tmp.clear_frame_cache();
    let mut session = EngineSession::new(
        fid,
        &mut resolver_tmp,
        ResolveTrace::new(ResolveLogLevel::Off),
    );
    let mut producers_ticked = BTreeSet::new();
    let time_s = eng.frame_time.total_ms as f32 / 1000.0;
    let mut host = EngineResolveHost {
        tree: &mut eng.tree,
        artifacts: &eng.artifacts,
        producers_ticked: &mut producers_ticked,
        runtime_buffers: &mut eng.runtime_buffers,
        slot_shapes: &eng.slot_shapes,
        graphics: eng.graphics.clone(),
        frame_time_seconds: time_s,
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
        EngineTestBuilder, bus, literal, output, path, produced_slot, trace_has_value_origin_path,
    };
    use crate::render_product::RenderProduct;
    use crate::runtime_buffer::RuntimeBuffer;
    use crate::runtime_product::RuntimeProduct;

    #[test]
    fn engine_new_has_frame_state_empty_bindings_resolver_and_tree_root() {
        let eng = Engine::new(TreePath::parse("/show.t").expect("path"));
        assert_eq!(eng.revision(), Revision::default());
        assert_eq!(eng.frame_time(), FrameTime::zero());
        assert!(eng.tree().bindings().next().is_none());
        assert!(eng.resolver().cache().is_empty());
        assert_eq!(eng.tree().len(), 1);
    }

    #[test]
    fn tick_advances_frame_num_revision_and_accumulates_frame_time() {
        let mut eng = Engine::new(TreePath::parse("/show.t").expect("path"));
        let initial_revision = eng.revision();
        eng.tick(10).expect("tick");
        assert_eq!(eng.frame_num(), FrameNum::new(1));
        assert!(eng.revision() > initial_revision);
        assert_eq!(eng.frame_time().delta_ms, 10);
        assert_eq!(eng.frame_time().total_ms, 10);
        let first_tick_revision = eng.revision();
        eng.tick(5).expect("tick");
        assert_eq!(eng.frame_num(), FrameNum::new(2));
        assert!(eng.revision() > first_tick_revision);
        assert_eq!(eng.frame_time().total_ms, 15);
    }

    #[test]
    fn fixture_resolves_shader_output_through_bus() {
        let mut h = EngineTestBuilder::new()
            .shader("shader", output("outputs[0]", 0.75))
            .fixture("fixture")
            .output_node("output")
            .bind_bus("video_out", produced_slot("shader", "outputs[0]"))
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
            .bind_bus("video", produced_slot("shader", "outputs[0]"))
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
    fn produced_slot_scalar_resolves_via_runtime_state_slots() {
        let mut h = EngineTestBuilder::new()
            .shader("shader", output("outputs[0]", 2.0))
            .build();

        let out = path("outputs[0]");
        let shader = h.node("shader");
        let a = h
            .resolve(QueryKey::ProducedSlot {
                node: shader,
                slot: out,
            })
            .expect("resolve");
        assert!(a.as_value().expect("value").eq(&LpsValueF32::F32(2.0)));
    }

    #[test]
    fn producer_runs_once_when_demanded_twice_in_same_frame() {
        let mut h = EngineTestBuilder::new()
            .shader("shader", output("outputs[0]", 2.0))
            .build();
        h.reset_shader_ticks("shader");
        let out = path("outputs[0]");
        let key = QueryKey::ProducedSlot {
            node: h.node("shader"),
            slot: out,
        };

        let (first, second) = super::resolve_twice_same_frame_with_engine_host(&mut h.engine, key)
            .expect("resolve pair");
        assert!(
            first
                .as_value()
                .expect("value")
                .eq(second.as_value().expect("value"))
        );
        assert_eq!(first.product.changed_at(), second.product.changed_at());

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

        assert!(pv.as_value().expect("value").eq(&LpsValueF32::F32(0.9)));
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
            .bind_bus("video", produced_slot("shader", "outputs[0]"))
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
    fn node_tree_binding_versions_are_available_for_debug_list() {
        let h = EngineTestBuilder::new()
            .shader("shader", output("outputs[0]", 0.5))
            .fixture("fixture")
            .bind_bus("video", produced_slot("shader", "outputs[0]"))
            .bind_demand_input("fixture", bus("video"))
            .build();
        let versions: Vec<_> = h.engine.tree().bindings().map(|e| e.version).collect();

        assert_eq!(versions, alloc::vec![Revision::new(1), Revision::new(1)]);
    }

    #[test]
    fn runtime_product_render_handle_is_node_owned_value() {
        let product = RenderProduct::new(NodeId::new(7), 0);
        let runtime = RuntimeProduct::render(product);
        assert_eq!(runtime.as_render(), Some(product));
        assert!(runtime.as_value().is_none());
        assert!(runtime.as_buffer().is_none());
    }

    #[test]
    fn runtime_buffer_inserted_via_engine_store_round_trips() {
        let mut engine = Engine::new(TreePath::parse("/show.t").expect("path"));
        let payload = RuntimeBuffer::raw(alloc::vec![0xaa, 0xbb]);
        let frame = Revision::new(4);
        let id = engine
            .runtime_buffers_mut()
            .insert(WithRevision::new(frame, payload.clone()));
        let buffers = engine.runtime_buffers();
        let got = buffers.get(id).expect("inserted buffer");
        assert_eq!(got.changed_at(), frame);
        assert_eq!(got.value(), &payload);
    }
}
