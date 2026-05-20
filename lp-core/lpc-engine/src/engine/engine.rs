//! [`Engine`] — owns spine state and mediates [`ResolveHost`] production for produced slots.

use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use hashbrown::HashMap;

use lpc_model::{
    ArtifactPath, BindingDef, ButtonDef, ClockDef, ComputeShaderDef, ControlMessage,
    ControlProduct, ControlRadioDef, Dim2u, FixtureDef, FluidDef, FluidEmitter, GlslOpts, NodeDef,
    NodeId, OutputDef, OutputDriverOptionsConfig, PlaylistDef, PlaylistEntry, PositiveF32,
    ProjectDef, Ratio, RenderOrder, Revision, ScalarHint, ShaderDef, ShaderParamDef, ShaderSlotDef,
    ShaderSlotMappingDef, SlotAccess, SlotAccessor, SlotData, SlotDirection, SlotMerge, SlotPath,
    SlotPathSegment, SlotSemantics, SlotShape, SlotShapeRegistry, SourcePath, StaticSlotShape,
    TextureDef, TreePath, WithRevision, Xy, advance_revision, current_revision,
    lookup_slot_data_and_shape,
};
use lpc_shared::time::TimeProvider;
use lpc_wire::WireNodeStatus;
use lpfs::FsChange;
use lpfs::lp_path::{LpPath, LpPathBuf};

use crate::artifact::{ArtifactState, ArtifactStore};
use crate::dataflow::binding::{BindingDraft, BindingError, BindingRef};
use crate::dataflow::resolver::{
    EngineSession, Production, ProductionSource, QueryKey, ResolveHost, ResolveLogLevel,
    ResolveTrace, Resolver, SessionHostResolver, SessionResolveError, TickResolver,
};
use crate::gfx::LpGraphics;
use crate::node::NodeEntry;
use crate::node::catch_node_panic::catch_node_panic;
use crate::node::{
    ControlRenderContext, ControlRenderServices, NodeCall, NodeCallKey, NodeError,
    NodeResourceInitContext, NodeRuntime, RenderContext, TickContext, VisualRenderServices,
};
use crate::node::{NodeEntryState, NodeTree};
use crate::products::control::{ControlLayout, ControlRenderRequest, ControlRenderTarget};
use crate::products::visual::{
    RenderTextureRequest, TextureRenderProduct, VisualProduct, VisualSampleBufferRequest,
    VisualSampleTarget,
};
use crate::resource::{RuntimeBufferId, RuntimeBufferStore};

use super::{ButtonService, EngineError, EngineServices, RadioService};
use super::{FrameNum, FrameTime};

/// Conventional demand input used by the M2 engine slice.
pub(crate) fn default_demand_input_path() -> SlotPath {
    SlotPath::parse("in").expect("default demand input slot path")
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
    services: EngineServices,
    artifact_nodes: HashMap<String, NodeId>,
    demand_roots: Vec<NodeId>,
    graphics: Option<Arc<dyn LpGraphics>>,
}

impl Engine {
    pub fn new(root_path: TreePath) -> Self {
        Self::with_services(root_path.clone(), EngineServices::new(root_path))
    }

    pub fn with_services(root_path: TreePath, services: EngineServices) -> Self {
        let revision = Revision::default();
        let mut slot_shapes = SlotShapeRegistry::default();
        register_authored_slot_shapes(&mut slot_shapes)
            .expect("authored slot shapes register without conflicts");
        Self {
            frame_num: FrameNum::default(),
            revision,
            frame_time: FrameTime::zero(),
            tree: NodeTree::new(root_path.clone(), revision),
            resolver: Resolver::new(),
            slot_shapes,
            runtime_buffers: RuntimeBufferStore::new(),
            artifacts: ArtifactStore::new(),
            services,
            artifact_nodes: HashMap::new(),
            demand_roots: Vec::new(),
            graphics: None,
        }
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub(super) fn set_revision(&mut self, revision: Revision) {
        self.revision = revision;
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

    pub fn services(&self) -> &EngineServices {
        &self.services
    }

    pub fn services_mut(&mut self) -> &mut EngineServices {
        &mut self.services
    }

    /// Engine [`NodeId`] for a node artifact path, if loaded.
    pub fn artifact_node_id(&self, path: &LpPath) -> Option<NodeId> {
        self.artifact_nodes.get(path.as_str()).copied()
    }

    pub(crate) fn insert_artifact_node(&mut self, path: LpPathBuf, id: NodeId) {
        self.artifact_nodes.insert(String::from(path.as_str()), id);
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
        let mut ctx = NodeResourceInitContext::new(id, &mut self.runtime_buffers);
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
        lp_perf::emit_begin!(lp_perf::EVENT_FRAME);
        let result = (|| {
            self.tick_nodes(delta_ms)?;
            let revision = self.revision;
            self.refresh_output_sink_configs();
            let buffers = &self.runtime_buffers;
            self.services
                .flush_dirty_output_sinks(revision, buffers)
                .map_err(|e| EngineError::OutputFlush {
                    message: alloc::format!("{e}"),
                })?;
            Ok(())
        })();
        lp_perf::emit_end!(lp_perf::EVENT_FRAME);
        result
    }

    fn refresh_output_sink_configs(&mut self) {
        let mut updates = Vec::new();
        for entry in self.tree.entries() {
            let Some(buffer_id) = self.runtime_output_sink_buffer_id(entry.id) else {
                continue;
            };
            let Some(NodeDef::Output(def)) = self.loaded_node_def(entry.artifact()) else {
                continue;
            };
            updates.push((buffer_id, def.clone()));
        }

        for (buffer_id, def) in updates {
            self.services.update_output_sink_config(buffer_id, &def);
        }
    }

    fn tick_nodes(&mut self, delta_ms: u32) -> Result<(), EngineError> {
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
        let time_provider = self.services.time_provider();
        let button_service = self.services.button_service();
        let radio_service = self.services.radio_service();
        let mut host = EngineResolveHost {
            tree: &mut self.tree,
            artifacts: &self.artifacts,
            producers_ticked: &mut producers_ticked,
            runtime_buffers: &mut self.runtime_buffers,
            slot_shapes: &self.slot_shapes,
            graphics: self.graphics.clone(),
            time_provider,
            button_service,
            radio_service,
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

    /// Accept filesystem changes for direct engine embedders.
    ///
    /// The server-owned project wrapper currently reloads the project from its
    /// filesystem on changes so node definition and shader source updates use
    /// the same loader path as initial load.
    pub fn handle_fs_changes(&mut self, _changes: &[FsChange]) -> Result<(), EngineError> {
        Ok(())
    }

    pub(crate) fn render_texture_product(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, SessionResolveError> {
        let mut producers_ticked = BTreeSet::new();
        let time_s = self.frame_time.total_ms as f32 / 1000.0;
        let time_provider = self.services.time_provider();
        let button_service = self.services.button_service();
        let radio_service = self.services.radio_service();
        let mut host = EngineResolveHost {
            tree: &mut self.tree,
            artifacts: &self.artifacts,
            producers_ticked: &mut producers_ticked,
            runtime_buffers: &mut self.runtime_buffers,
            slot_shapes: &self.slot_shapes,
            graphics: self.graphics.clone(),
            time_provider,
            button_service,
            radio_service,
            frame_time_seconds: time_s,
        };
        host.render_node_texture(product, request)
    }

    #[cfg(test)]
    pub(crate) fn render_texture_for_test(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, SessionResolveError> {
        self.render_texture_product(product, request)
    }

    #[cfg(test)]
    pub(crate) fn render_control_for_test(
        &mut self,
        product: ControlProduct,
        request: &ControlRenderRequest,
        target: ControlRenderTarget<'_>,
    ) -> Result<ControlLayout, SessionResolveError> {
        let mut producers_ticked = BTreeSet::new();
        let time_s = self.frame_time.total_ms as f32 / 1000.0;
        let time_provider = self.services.time_provider();
        let button_service = self.services.button_service();
        let radio_service = self.services.radio_service();
        let mut host = EngineResolveHost {
            tree: &mut self.tree,
            artifacts: &self.artifacts,
            producers_ticked: &mut producers_ticked,
            runtime_buffers: &mut self.runtime_buffers,
            slot_shapes: &self.slot_shapes,
            graphics: self.graphics.clone(),
            time_provider,
            button_service,
            radio_service,
            frame_time_seconds: time_s,
        };
        host.render_node_control(product, request, target)
    }

    fn consumed_slot_is_bound(&self, node: NodeId, slot: &SlotPath) -> bool {
        self.tree.binding_for_consumed_slot(node, slot).is_some()
    }
}

fn register_authored_slot_shapes(
    registry: &mut SlotShapeRegistry,
) -> Result<(), lpc_model::SlotShapeRegistryError> {
    for id in [
        BindingDef::SHAPE_ID,
        ControlMessage::SHAPE_ID,
        lpc_model::NodeArtifact::SHAPE_ID,
        ButtonDef::SHAPE_ID,
        ClockDef::SHAPE_ID,
        FixtureDef::SHAPE_ID,
        FluidDef::SHAPE_ID,
        FluidEmitter::SHAPE_ID,
        OutputDef::SHAPE_ID,
        OutputDriverOptionsConfig::SHAPE_ID,
        PlaylistDef::SHAPE_ID,
        PlaylistEntry::SHAPE_ID,
        ProjectDef::SHAPE_ID,
        ControlRadioDef::SHAPE_ID,
        ComputeShaderDef::SHAPE_ID,
        GlslOpts::SHAPE_ID,
        ScalarHint::SHAPE_ID,
        ShaderDef::SHAPE_ID,
        ShaderParamDef::SHAPE_ID,
        ShaderSlotDef::SHAPE_ID,
        ShaderSlotMappingDef::SHAPE_ID,
        TextureDef::SHAPE_ID,
        ArtifactPath::SHAPE_ID,
        Dim2u::SHAPE_ID,
        PositiveF32::SHAPE_ID,
        Ratio::SHAPE_ID,
        RenderOrder::SHAPE_ID,
        SourcePath::SHAPE_ID,
        Xy::SHAPE_ID,
    ] {
        lpc_model::slot_shapes::ensure_static_slot_shape(registry, id)?;
    }
    Ok(())
}

/// Host adapter with borrows disjoint from the [`Resolver`] handed to [`EngineSession`].
struct EngineResolveHost<'a> {
    tree: &'a mut NodeTree<Box<dyn NodeRuntime>>,
    artifacts: &'a ArtifactStore,
    producers_ticked: &'a mut BTreeSet<NodeId>,
    runtime_buffers: &'a mut RuntimeBufferStore,
    slot_shapes: &'a SlotShapeRegistry,
    graphics: Option<Arc<dyn LpGraphics>>,
    time_provider: Option<Rc<dyn TimeProvider>>,
    button_service: Option<Rc<dyn ButtonService>>,
    radio_service: Option<Rc<dyn RadioService>>,
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
        let time_provider = self.time_provider.clone();
        let button_service = self.button_service.clone();
        let radio_service = self.radio_service.clone();
        let time_s = self.frame_time_seconds;
        let slot_shapes = self.slot_shapes;
        let tick_result = {
            let mut bridge = SessionHostResolver {
                session,
                host: self as &mut dyn ResolveHost,
            };
            let resolver_dyn: &mut dyn TickResolver = &mut bridge;
            let mut tick_ctx = TickContext::with_engine_services(
                node_id,
                revision,
                artifact_id,
                content_frame,
                resolver_dyn,
                slot_shapes,
                gfx,
                time_provider,
                button_service,
                radio_service,
                time_s,
            );
            catch_node_panic(|| node_runtime.tick(&mut tick_ctx))
        };

        let entry = self.tree.get_mut(node_id).ok_or_else(|| {
            SessionResolveError::other(format!("produce: unknown node {node_id:?}"))
        })?;
        entry.set_state(NodeEntryState::Alive(node_runtime), restore_frame);

        match tick_result {
            Ok(()) => {
                set_entry_status_if_changed(entry, WireNodeStatus::Ok, revision);
                self.producers_ticked.insert(node_id);
                Ok(())
            }
            Err(e) => {
                let message = e.to_string();
                set_entry_status_if_changed(
                    entry,
                    WireNodeStatus::Error(message.clone()),
                    revision,
                );
                Err(SessionResolveError::other(format!(
                    "produce: tick failed: {message}"
                )))
            }
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
    ) -> Option<(BindingRef, crate::dataflow::binding::BindingEntry)> {
        self.tree
            .binding_for_consumed_slot(node, slot)
            .map(|(binding_ref, entry)| (binding_ref, entry.clone()))
    }

    fn bindings_for_consumed_slot(
        &self,
        node: NodeId,
        slot: &SlotPath,
    ) -> Vec<(BindingRef, crate::dataflow::binding::BindingEntry)> {
        self.tree
            .bindings_for_consumed_slot(node, slot)
            .into_iter()
            .map(|(binding_ref, entry)| (binding_ref, entry.clone()))
            .collect()
    }

    fn providers_for_bus(
        &self,
        channel: &lpc_model::ChannelName,
    ) -> Vec<(BindingRef, crate::dataflow::binding::BindingEntry)> {
        self.tree
            .providers_for_bus(channel)
            .into_iter()
            .map(|(binding_ref, entry)| (binding_ref, entry.clone()))
            .collect()
    }

    fn merge_policy_for_consumed_slot(&self, node: NodeId, slot: &SlotPath) -> SlotMerge {
        let Some(entry) = self.tree.get(node) else {
            return SlotMerge::Latest;
        };
        if let Ok(Some(policy)) =
            self.read_shader_consumed_slot_merge_policy(&entry.def_handle, slot)
        {
            return policy;
        }
        self.read_authored_def_slot_semantics(&entry.def_handle, slot)
            .ok()
            .filter(|semantics| semantics.direction == SlotDirection::Consumed)
            .map_or(SlotMerge::Latest, |semantics| semantics.merge)
    }

    fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, SessionResolveError> {
        self.render_node_texture(product, request)
    }

    fn render_control(
        &mut self,
        product: ControlProduct,
        request: &ControlRenderRequest,
        target: ControlRenderTarget<'_>,
    ) -> Result<ControlLayout, SessionResolveError> {
        self.render_node_control(product, request, target)
    }

    fn runtime_buffer_mut(
        &mut self,
        id: crate::resource::RuntimeBufferId,
        frame: Revision,
    ) -> Result<&mut crate::resource::RuntimeBuffer, SessionResolveError> {
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
    ) -> Result<SlotData, SessionResolveError> {
        let state = node.runtime_state_slots().ok_or_else(|| {
            SessionResolveError::other("node does not expose runtime state slots")
        })?;
        let (data, shape) = lookup_slot_data_and_shape(state, self.slot_shapes, slot)
            .map_err(|e| SessionResolveError::other(format!("runtime state lookup: {e}")))?;
        Ok(lpc_wire::snapshot_slot_shape(shape, data, self.slot_shapes))
    }

    fn read_authored_def_product(
        &self,
        handle: &crate::node::NodeDefHandle,
        slot: &SlotPath,
    ) -> Result<SlotData, SessionResolveError> {
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
        let (data, shape) = lookup_slot_data_and_shape(def, self.slot_shapes, slot)
            .map_err(|e| SessionResolveError::other(format!("authored def lookup: {e}")))?;
        Ok(lpc_wire::snapshot_slot_shape(shape, data, self.slot_shapes))
    }

    fn read_authored_def_product_by_accessor(
        &self,
        handle: &crate::node::NodeDefHandle,
        accessor: &SlotAccessor,
    ) -> Result<SlotData, SessionResolveError> {
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
        let (_, shape) = lookup_slot_data_and_shape(def, self.slot_shapes, accessor.path())
            .map_err(|e| SessionResolveError::other(format!("authored def accessor shape: {e}")))?;
        Ok(lpc_wire::snapshot_slot_shape(shape, data, self.slot_shapes))
    }

    fn read_shader_consumed_slot_merge_policy(
        &self,
        handle: &crate::node::NodeDefHandle,
        slot: &SlotPath,
    ) -> Result<Option<SlotMerge>, SessionResolveError> {
        let Some(SlotPathSegment::Field(name)) = slot.segments().first() else {
            return Ok(None);
        };
        if slot.segments().len() != 1 {
            return Ok(None);
        }
        let def = self.loaded_node_def(handle)?;
        let shader_slot = match def {
            NodeDef::Shader(config) => config.consumed_slots.entries.get(name.as_str()),
            NodeDef::ComputeShader(config) => config.consumed_slots.entries.get(name.as_str()),
            _ => None,
        };
        Ok(shader_slot.map(|slot| match slot.kind.value() {
            lpc_model::ShaderSlotKind::Map => SlotMerge::ByKey,
            lpc_model::ShaderSlotKind::Value => SlotMerge::Latest,
        }))
    }

    fn read_authored_def_slot_semantics(
        &self,
        handle: &crate::node::NodeDefHandle,
        slot: &SlotPath,
    ) -> Result<SlotSemantics, SessionResolveError> {
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
        let shape = self.slot_shapes.get(&def.shape_id()).ok_or_else(|| {
            SessionResolveError::other(format!("missing node def shape {}", def.shape_id()))
        })?;
        slot_path_semantics(shape, self.slot_shapes, slot)
    }

    fn loaded_node_def(
        &self,
        handle: &crate::node::NodeDefHandle,
    ) -> Result<&NodeDef, SessionResolveError> {
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
        match &entry.state {
            ArtifactState::Loaded(def)
            | ArtifactState::Prepared(def)
            | ArtifactState::Idle(def) => Ok(def),
            other => Err(SessionResolveError::other(format!(
                "node def artifact {:?} has no loaded payload: {other:?}",
                handle.artifact()
            ))),
        }
    }

    fn render_node_texture(
        &mut self,
        product: VisualProduct,
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
                call: NodeCallKey::new(node_id, NodeCall::Visual { product }),
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
                        "node {node_id:?} cannot visual product output {}: NodeRuntime::render_node() returned None",
                        product.output()
                    )),
                );
            };
            let mut ctx = RenderContext::with_services(
                node_id,
                revision,
                self.graphics.clone(),
                self.time_provider.clone(),
                self.frame_time_seconds,
                self,
            );
            catch_node_panic(|| render_node.render_texture(product, request, &mut ctx))
        };

        let entry = self.tree.get_mut(node_id).ok_or_else(|| {
            SessionResolveError::other(format!("render: unknown node {node_id:?}"))
        })?;
        entry.set_state(NodeEntryState::Alive(node_runtime), revision);

        match result {
            Ok(product) => {
                set_entry_status_if_changed(entry, WireNodeStatus::Ok, revision);
                Ok(product)
            }
            Err(e) => {
                let message = e.to_string();
                set_entry_status_if_changed(
                    entry,
                    WireNodeStatus::Error(message.clone()),
                    revision,
                );
                Err(SessionResolveError::other(format!("render: {message}")))
            }
        }
    }

    fn render_node_texture_into(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        target: &mut lp_shader::LpsTextureBuf,
    ) -> Result<(), SessionResolveError> {
        let node_id = product.node();
        let revision = current_revision();
        let mut node_runtime = {
            let entry = self.tree.get_mut(node_id).ok_or_else(|| {
                SessionResolveError::other(format!("render: unknown node {node_id:?}"))
            })?;
            let old_changed_at = entry.state.changed_at();
            let executing = NodeEntryState::Executing {
                call: NodeCallKey::new(node_id, NodeCall::Visual { product }),
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
                return restore_node_after_failed_render_unit(
                    self.tree,
                    node_id,
                    node_runtime,
                    revision,
                    SessionResolveError::other(format!(
                        "node {node_id:?} cannot visual product output {}: NodeRuntime::render_node() returned None",
                        product.output()
                    )),
                );
            };
            let mut ctx = RenderContext::with_services(
                node_id,
                revision,
                self.graphics.clone(),
                self.time_provider.clone(),
                self.frame_time_seconds,
                self,
            );
            catch_node_panic(|| render_node.render_texture_into(product, request, target, &mut ctx))
        };

        let entry = self.tree.get_mut(node_id).ok_or_else(|| {
            SessionResolveError::other(format!("render: unknown node {node_id:?}"))
        })?;
        entry.set_state(NodeEntryState::Alive(node_runtime), revision);

        match result {
            Ok(()) => {
                set_entry_status_if_changed(entry, WireNodeStatus::Ok, revision);
                Ok(())
            }
            Err(e) => {
                let message = e.to_string();
                set_entry_status_if_changed(
                    entry,
                    WireNodeStatus::Error(message.clone()),
                    revision,
                );
                Err(SessionResolveError::other(format!("render: {message}")))
            }
        }
    }

    fn sample_node_visual_into(
        &mut self,
        product: VisualProduct,
        request: VisualSampleBufferRequest<'_>,
        target: VisualSampleTarget<'_>,
    ) -> Result<(), SessionResolveError> {
        let node_id = product.node();
        let revision = current_revision();
        let mut node_runtime = {
            let entry = self.tree.get_mut(node_id).ok_or_else(|| {
                SessionResolveError::other(format!("sample visual: unknown node {node_id:?}"))
            })?;
            let old_changed_at = entry.state.changed_at();
            let executing = NodeEntryState::Executing {
                call: NodeCallKey::new(node_id, NodeCall::Visual { product }),
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
                        "sample visual: node {node_id:?} not alive"
                    )));
                }
            }
        };

        let result = {
            let Some(render_node) = node_runtime.render_node() else {
                return restore_node_after_failed_render_unit(
                    self.tree,
                    node_id,
                    node_runtime,
                    revision,
                    SessionResolveError::other(format!(
                        "node {node_id:?} cannot sample visual product output {}: NodeRuntime::render_node() returned None",
                        product.output()
                    )),
                );
            };
            let mut ctx = RenderContext::with_services(
                node_id,
                revision,
                self.graphics.clone(),
                self.time_provider.clone(),
                self.frame_time_seconds,
                self,
            );
            catch_node_panic(|| render_node.sample_visual_into(product, request, target, &mut ctx))
        };

        let entry = self.tree.get_mut(node_id).ok_or_else(|| {
            SessionResolveError::other(format!("sample visual: unknown node {node_id:?}"))
        })?;
        entry.set_state(NodeEntryState::Alive(node_runtime), revision);

        match result {
            Ok(()) => {
                set_entry_status_if_changed(entry, WireNodeStatus::Ok, revision);
                Ok(())
            }
            Err(e) => {
                let message = e.to_string();
                set_entry_status_if_changed(
                    entry,
                    WireNodeStatus::Error(message.clone()),
                    revision,
                );
                Err(SessionResolveError::other(format!(
                    "sample visual: {message}"
                )))
            }
        }
    }

    fn render_node_control(
        &mut self,
        product: ControlProduct,
        request: &ControlRenderRequest,
        target: ControlRenderTarget<'_>,
    ) -> Result<ControlLayout, SessionResolveError> {
        let node_id = product.node();
        let revision = current_revision();
        let mut node_runtime = {
            let entry = self.tree.get_mut(node_id).ok_or_else(|| {
                SessionResolveError::other(format!("control render: unknown node {node_id:?}"))
            })?;
            let old_changed_at = entry.state.changed_at();
            let executing = NodeEntryState::Executing {
                call: NodeCallKey::new(node_id, NodeCall::Control { product }),
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
                        "control render: node {node_id:?} not alive"
                    )));
                }
            }
        };

        let result = {
            let Some(control_node) = node_runtime.control_node() else {
                return restore_node_after_failed_control(
                    self.tree,
                    node_id,
                    node_runtime,
                    revision,
                    SessionResolveError::other(format!(
                        "node {node_id:?} cannot render control product output {}: NodeRuntime::control_node() returned None",
                        product.output()
                    )),
                );
            };
            let mut ctx = ControlRenderContext::new(
                node_id,
                revision,
                self.graphics.clone(),
                self.frame_time_seconds,
                self,
            );
            catch_node_panic(|| control_node.render_control(product, request, target, &mut ctx))
        };

        let entry = self.tree.get_mut(node_id).ok_or_else(|| {
            SessionResolveError::other(format!("control render: unknown node {node_id:?}"))
        })?;
        entry.set_state(NodeEntryState::Alive(node_runtime), revision);

        match result {
            Ok(layout) => {
                set_entry_status_if_changed(entry, WireNodeStatus::Ok, revision);
                Ok(layout)
            }
            Err(e) => {
                let message = e.to_string();
                set_entry_status_if_changed(
                    entry,
                    WireNodeStatus::Error(message.clone()),
                    revision,
                );
                Err(SessionResolveError::other(format!(
                    "control render: {message}"
                )))
            }
        }
    }
}

fn slot_path_semantics(
    shape: &SlotShape,
    registry: &SlotShapeRegistry,
    slot: &SlotPath,
) -> Result<SlotSemantics, SessionResolveError> {
    slot_path_semantics_segments(shape, registry, slot, slot.segments())
}

fn slot_path_semantics_segments(
    shape: &SlotShape,
    registry: &SlotShapeRegistry,
    slot: &SlotPath,
    segments: &[SlotPathSegment],
) -> Result<SlotSemantics, SessionResolveError> {
    let shape = resolve_shape_projection(shape, registry)?;
    let Some((head, tail)) = segments.split_first() else {
        return Err(SessionResolveError::other(format!(
            "slot path {slot} does not identify a record field"
        )));
    };

    match (shape, head) {
        (SlotShape::Record { fields, .. }, SlotPathSegment::Field(name)) => {
            let field = fields
                .iter()
                .find(|field| field.name == *name)
                .ok_or_else(|| {
                    SessionResolveError::other(format!("node def has no slot field {name}"))
                })?;
            if tail.is_empty() {
                Ok(field.semantics)
            } else {
                slot_path_semantics_segments(&field.shape, registry, slot, tail)
            }
        }
        (SlotShape::Map { value, .. }, SlotPathSegment::Key(_)) => {
            slot_path_semantics_segments(value, registry, slot, tail)
        }
        (SlotShape::Option { some, .. }, SlotPathSegment::Field(name))
            if name.as_str() == "some" =>
        {
            slot_path_semantics_segments(some, registry, slot, tail)
        }
        (SlotShape::Enum { variants, .. }, SlotPathSegment::Field(name)) => {
            let variant = variants
                .iter()
                .find(|variant| variant.name == *name)
                .ok_or_else(|| {
                    SessionResolveError::other(format!("node def enum has no variant {name}"))
                })?;
            slot_path_semantics_segments(&variant.shape, registry, slot, tail)
        }
        (_, SlotPathSegment::Field(name)) => Err(SessionResolveError::other(format!(
            "slot path field {name} cannot descend through node def shape for {slot}"
        ))),
        (_, SlotPathSegment::Key(key)) => Err(SessionResolveError::other(format!(
            "slot path key {key:?} cannot descend through node def shape for {slot}"
        ))),
    }
}

fn resolve_shape_projection<'a>(
    shape: &'a SlotShape,
    registry: &'a SlotShapeRegistry,
) -> Result<&'a SlotShape, SessionResolveError> {
    let mut shape = shape;
    loop {
        match shape {
            SlotShape::Ref { id } => {
                shape = registry.get(id).ok_or_else(|| {
                    SessionResolveError::other(format!("missing referenced node def shape {id}"))
                })?;
            }
            SlotShape::Custom {
                shape: projected, ..
            } => {
                shape = projected;
            }
            other => return Ok(other),
        }
    }
}

impl ControlRenderServices for EngineResolveHost<'_> {
    fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, NodeError> {
        self.render_node_texture(product, request)
            .map_err(|e| NodeError::msg(format!("render texture: {e}")))
    }

    fn render_texture_into(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        target: &mut lp_shader::LpsTextureBuf,
    ) -> Result<(), NodeError> {
        self.render_node_texture_into(product, request, target)
            .map_err(|e| NodeError::msg(format!("render texture: {e}")))
    }

    fn sample_visual_into(
        &mut self,
        product: VisualProduct,
        request: VisualSampleBufferRequest<'_>,
        target: VisualSampleTarget<'_>,
    ) -> Result<(), NodeError> {
        self.sample_node_visual_into(product, request, target)
            .map_err(|e| NodeError::msg(format!("sample visual: {e}")))
    }
}

impl VisualRenderServices for EngineResolveHost<'_> {
    fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, NodeError> {
        self.render_node_texture(product, request)
            .map_err(|e| NodeError::msg(format!("render texture: {e}")))
    }

    fn render_texture_into(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        target: &mut lp_shader::LpsTextureBuf,
    ) -> Result<(), NodeError> {
        self.render_node_texture_into(product, request, target)
            .map_err(|e| NodeError::msg(format!("render texture: {e}")))
    }

    fn sample_visual_into(
        &mut self,
        product: VisualProduct,
        request: VisualSampleBufferRequest<'_>,
        target: VisualSampleTarget<'_>,
    ) -> Result<(), NodeError> {
        self.sample_node_visual_into(product, request, target)
            .map_err(|e| NodeError::msg(format!("sample visual: {e}")))
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

fn set_entry_status_if_changed<N>(
    entry: &mut NodeEntry<N>,
    status: WireNodeStatus,
    revision: Revision,
) {
    if entry.status.value() != &status {
        entry.set_status(status, revision);
    }
}

fn restore_node_after_failed_render_unit(
    tree: &mut NodeTree<Box<dyn NodeRuntime>>,
    node_id: NodeId,
    node_runtime: Box<dyn NodeRuntime>,
    revision: Revision,
    err: SessionResolveError,
) -> Result<(), SessionResolveError> {
    if let Some(entry) = tree.get_mut(node_id) {
        entry.set_state(NodeEntryState::Alive(node_runtime), revision);
    }
    Err(err)
}

fn restore_node_after_failed_control(
    tree: &mut NodeTree<Box<dyn NodeRuntime>>,
    node_id: NodeId,
    node_runtime: Box<dyn NodeRuntime>,
    revision: Revision,
    err: SessionResolveError,
) -> Result<ControlLayout, SessionResolveError> {
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
    let time_provider = host.time_provider.clone();
    let button_service = host.button_service.clone();
    let radio_service = host.radio_service.clone();
    let time_s = host.frame_time_seconds;
    let slot_shapes = host.slot_shapes;
    let tick_result = {
        let mut bridge = SessionHostResolver {
            session,
            host: host as &mut dyn ResolveHost,
        };
        let resolver_dyn: &mut dyn TickResolver = &mut bridge;
        let mut tick_ctx = TickContext::with_engine_services(
            node_id,
            revision,
            artifact_id,
            content_frame,
            resolver_dyn,
            slot_shapes,
            gfx,
            time_provider,
            button_service,
            radio_service,
            time_s,
        );
        catch_node_panic(|| node_runtime.tick(&mut tick_ctx))
    };

    let entry = host
        .tree
        .get_mut(node_id)
        .ok_or(EngineError::UnknownNode(node_id))?;
    entry.set_state(NodeEntryState::Alive(node_runtime), restore_frame);

    match tick_result {
        Ok(()) => {
            set_entry_status_if_changed(entry, WireNodeStatus::Ok, revision);
            Ok(())
        }
        Err(e) => {
            let message = e.to_string();
            set_entry_status_if_changed(entry, WireNodeStatus::Error(message.clone()), revision);
            Err(EngineError::Node {
                node: node_id,
                message,
            })
        }
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
    let time_provider = eng.services.time_provider();
    let button_service = eng.services.button_service();
    let radio_service = eng.services.radio_service();
    let mut host = EngineResolveHost {
        tree: &mut eng.tree,
        artifacts: &eng.artifacts,
        producers_ticked: &mut producers_ticked,
        runtime_buffers: &mut eng.runtime_buffers,
        slot_shapes: &eng.slot_shapes,
        graphics: eng.graphics.clone(),
        time_provider,
        button_service,
        radio_service,
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
    let time_provider = eng.services.time_provider();
    let button_service = eng.services.button_service();
    let radio_service = eng.services.radio_service();
    let mut host = EngineResolveHost {
        tree: &mut eng.tree,
        artifacts: &eng.artifacts,
        producers_ticked: &mut producers_ticked,
        runtime_buffers: &mut eng.runtime_buffers,
        slot_shapes: &eng.slot_shapes,
        graphics: eng.graphics.clone(),
        time_provider,
        button_service,
        radio_service,
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

    use crate::engine::test_support::{
        EngineTestBuilder, bus, literal, output, path, produced_slot, trace_has_value_origin_path,
    };
    use crate::node::test_placeholder_spine;
    use crate::products::visual::VisualProduct;
    use crate::resource::RuntimeBuffer;
    use lpc_wire::{WireChildKind, WireSlotIndex};

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
    fn tick_error_sets_node_status_and_restores_runtime() {
        let mut eng = Engine::new(TreePath::parse("/show.t").expect("path"));
        let root = eng.tree().root();
        let (cfg, artifact) = test_placeholder_spine();
        let node = eng
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("bad").expect("name"),
                lpc_model::NodeName::parse("shader").expect("kind"),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg,
                artifact,
                Revision::new(1),
            )
            .expect("add node");
        eng.attach_runtime_node(node, Box::new(FailingNode), Revision::new(1))
            .expect("attach node");
        eng.add_binding(
            crate::dataflow::binding::BindingDraft {
                source: crate::dataflow::binding::BindingSource::Literal(lpc_model::LpValue::F32(
                    1.0,
                )),
                target: crate::dataflow::binding::BindingTarget::ConsumedSlot {
                    node,
                    slot: default_demand_input_path(),
                },
                priority: crate::dataflow::binding::BindingPriority::new(0),
                kind: lpc_model::Kind::Color,
                owner: node,
            },
            Revision::new(1),
        )
        .expect("bind demand input");
        eng.add_demand_root(node);

        let err = eng.tick(10).expect_err("tick should fail");
        assert!(err.to_string().contains("intentional tick failure"));

        let entry = eng.tree().get(node).expect("entry");
        assert!(matches!(entry.state.value(), NodeEntryState::Alive(_)));
        assert!(matches!(
            entry.status.value(),
            WireNodeStatus::Error(message) if message == "intentional tick failure"
        ));
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
                .eq(&second.as_value().expect("value"))
        );
        assert_eq!(
            first.value_leaf().expect("value").changed_at(),
            second.value_leaf().expect("value").changed_at()
        );

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
    fn equal_priority_bus_bindings_are_ambiguous_when_resolved_directly() {
        let mut h = EngineTestBuilder::new()
            .bind_bus_with_priority("video", literal(0.25), 7)
            .expect("first binding")
            .bind_bus_with_priority("video", literal(0.9), 7)
            .expect("second binding")
            .build();

        assert!(matches!(
            h.resolve_bus("video"),
            Err(SessionResolveError::AmbiguousBusBinding { .. })
        ));
    }

    #[test]
    fn nested_consumed_slot_semantics_reach_playlist_entry_trigger() {
        let mut registry = SlotShapeRegistry::default();
        lpc_model::slot_shapes::register_all_static_slot_shapes(&mut registry).expect("shapes");
        let semantics = slot_path_semantics(
            &<lpc_model::PlaylistDef as lpc_model::StaticSlotShape>::slot_shape(),
            &registry,
            &SlotPath::parse("entries[2].trigger").expect("trigger path"),
        )
        .expect("trigger semantics");

        assert_eq!(semantics.direction, SlotDirection::Consumed);
        assert_eq!(semantics.merge, SlotMerge::ByKey);
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
    fn visual_product_handle_is_node_owned_value() {
        let product = VisualProduct::new(NodeId::new(7), 0);
        let value = lpc_model::LpValue::Product(lpc_model::ProductRef::visual(product));
        assert_eq!(
            value,
            lpc_model::LpValue::Product(lpc_model::ProductRef::Visual(product))
        );
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

    struct FailingNode;

    impl NodeRuntime for FailingNode {
        fn tick(&mut self, _ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
            Err(NodeError::msg("intentional tick failure"))
        }

        fn destroy(&mut self, _ctx: &mut crate::node::DestroyCtx<'_>) -> Result<(), NodeError> {
            Ok(())
        }

        fn handle_memory_pressure(
            &mut self,
            _level: crate::node::PressureLevel,
            _ctx: &mut crate::node::MemPressureCtx<'_>,
        ) -> Result<(), NodeError> {
            Ok(())
        }
    }
}
