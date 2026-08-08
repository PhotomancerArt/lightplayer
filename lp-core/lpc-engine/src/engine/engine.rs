//! [`Engine`] — owns spine state and mediates [`ResolveHost`] production for produced slots.

use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use lp_collection::VecSet;
use lpc_model::{
    ChannelName, ControlProduct, NodeDef, NodeDefLocation, NodeDefState, NodeId, Revision,
    SlotAccess, SlotAccessor, SlotData, SlotDirection, SlotMerge, SlotPath, SlotPathSegment,
    SlotSemantics, SlotShapeLookup, SlotShapeRegistry, SlotShapeView, TreePath, WithRevision,
    advance_revision, lookup_slot_data_and_shape,
};
use lpc_registry::ProjectRegistry;
use lpc_shared::time::TimeProvider;
use lpc_wire::{ControlDisplayLayoutProbeResult, ControlDisplayLayoutRead, NodeRuntimeStatus};

use crate::dataflow::binding::{BindingDraft, BindingError, BindingRef};
use crate::dataflow::resolver::{
    EngineSession, Production, ProductionSource, QueryKey, ResolveHost, ResolveLogLevel,
    ResolveTrace, Resolver, SessionHostResolver, SessionResolveError, TickResolver,
};
use crate::node::RuntimeNodeEntry;
use crate::node::catch_node_panic::catch_node_panic_framed;
use crate::node::{
    ControlRenderContext, ControlRenderServices, NodeCall, NodeCallKey, NodeError,
    NodeResourceInitContext, NodeRuntime, ProduceResult, RenderContext, TickContext,
    VisualRenderServices,
};
use crate::node::{NodeEntryState, RuntimeNodeTree};
use crate::products::control::{ControlLayout, ControlRenderRequest, ControlRenderTarget};
use crate::products::visual::{
    ProductSpaceInfo, RenderTextureRequest, TextureRenderProduct, VisualProduct,
    VisualSampleBufferRequest, VisualSampleTarget,
};
use crate::resource::{RuntimeBufferId, RuntimeBufferStore};
use lp_gfx::{LpGraphics, TextureHandle};

use super::{ButtonService, EngineError, EngineServices, ProjectRuntimeIndex, RadioService};
use super::{FrameNum, FrameTime};

/// Conventional demand input used by the M2 engine slice.
#[cfg(test)]
pub(crate) fn default_demand_input_path() -> SlotPath {
    SlotPath::parse("in").expect("default demand input slot path")
}

/// Core runtime owner for the demand-driven spine (M2).
pub struct Engine {
    frame_num: FrameNum,
    revision: Revision,
    frame_time: FrameTime,
    tree: RuntimeNodeTree<Box<dyn NodeRuntime>>,
    resolver: Resolver,
    slot_shapes: SlotShapeRegistry,
    runtime_buffers: RuntimeBufferStore,
    project_runtime_index: ProjectRuntimeIndex,
    services: EngineServices,
    demand_roots: Vec<NodeId>,
    graphics: Option<Arc<dyn LpGraphics>>,
    /// Engaged panel writers (panel.md P1–P4). A side store on purpose:
    /// `apply_project_changes` rebuilds bindings from defs and must never
    /// destroy an engaged control.
    panel_writers: crate::dataflow::panel_writers::PanelWriterStore,
    /// Published timebases and their phasor integrators, keyed by the node
    /// that produces each (`dataflow::timebase`). A side store for the same
    /// reason panel writers are one — it is Engine state that must outlive
    /// `apply_project_changes` — and additionally because servicing a
    /// per-uniform phasor read through node dispatch would put the
    /// resolver's heaviest machinery on the hottest new path.
    timebases: crate::dataflow::timebase::TimebaseStore,
    /// Device-level safe-mode output ceiling, Q16 (`None` = no clamp).
    ///
    /// DEVICE state, not project data: set by the embedder (firmware, from a
    /// consumed boot-control record), never by anything a project can touch.
    /// That separation is the point — the mechanism that saves a device must
    /// not be editable by the thing that broke it. Composed into every
    /// fixture's power scale via `min` in the fixture render.
    safe_output_clamp_q16: Option<u32>,
    /// The tree shape and resolver epoch as of the last tick, so that a
    /// structural change that forgot to invalidate resolution is caught here
    /// rather than by someone noticing a stale value on a device.
    #[cfg(debug_assertions)]
    last_structural_check: Option<((usize, usize, Revision), u64)>,
}

impl Engine {
    pub fn new(root_path: TreePath) -> Self {
        Self::with_services(root_path.clone(), EngineServices::new(root_path))
    }

    pub fn with_services(root_path: TreePath, services: EngineServices) -> Self {
        let revision = Revision::default();
        let slot_shapes = SlotShapeRegistry::default();
        Self {
            frame_num: FrameNum::default(),
            revision,
            frame_time: FrameTime::zero(),
            tree: RuntimeNodeTree::new(root_path.clone(), revision),
            resolver: Resolver::new(),
            slot_shapes,
            runtime_buffers: RuntimeBufferStore::new(),
            project_runtime_index: ProjectRuntimeIndex::new(),
            services,
            demand_roots: Vec::new(),
            graphics: None,
            panel_writers: crate::dataflow::panel_writers::PanelWriterStore::new(),
            timebases: crate::dataflow::timebase::TimebaseStore::new(),
            safe_output_clamp_q16: None,
            #[cfg(debug_assertions)]
            last_structural_check: None,
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

    pub fn tree(&self) -> &RuntimeNodeTree<Box<dyn NodeRuntime>> {
        &self.tree
    }

    pub fn tree_mut(&mut self) -> &mut RuntimeNodeTree<Box<dyn NodeRuntime>> {
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

    pub fn project_runtime_index(&self) -> &ProjectRuntimeIndex {
        &self.project_runtime_index
    }

    pub(crate) fn project_runtime_index_mut(&mut self) -> &mut ProjectRuntimeIndex {
        &mut self.project_runtime_index
    }

    pub fn services(&self) -> &EngineServices {
        &self.services
    }

    pub fn services_mut(&mut self) -> &mut EngineServices {
        &mut self.services
    }

    pub fn demand_roots(&self) -> &[NodeId] {
        &self.demand_roots
    }

    pub fn add_demand_root(&mut self, node: NodeId) {
        self.demand_roots.push(node);
    }

    pub(crate) fn remove_runtime_subtree(
        &mut self,
        node: NodeId,
        frame: Revision,
    ) -> Result<(), EngineError> {
        if node == self.tree.root() {
            return Err(EngineError::Tree(crate::node::TreeError::RootMutation));
        }
        let ids = self.tree.subtree_ids_depth_first(node)?;
        for &id in &ids {
            self.cleanup_runtime_node(id, frame)?;
            self.project_runtime_index.remove_runtime_node(id);
        }
        self.demand_roots.retain(|root| !ids.contains(root));
        self.tree.remove_subtree(node, frame)?;
        self.resolver.invalidate_structure();
        Ok(())
    }

    pub(crate) fn reattach_runtime_node(
        &mut self,
        node: NodeId,
        runtime: Box<dyn NodeRuntime>,
        frame: Revision,
    ) -> Result<(), EngineError> {
        self.cleanup_runtime_node(node, frame)?;
        self.attach_runtime_node(node, runtime, frame)?;
        self.resolver.invalidate_structure();
        Ok(())
    }

    fn cleanup_runtime_node(&mut self, node: NodeId, frame: Revision) -> Result<(), EngineError> {
        let sink = self.runtime_output_sink_buffer_id(node);
        if let Some(sink) = sink {
            self.services.unregister_output_sink(sink);
        }

        let state = {
            let entry = self
                .tree
                .get_mut(node)
                .ok_or(EngineError::UnknownNode(node))?;
            let old_changed_at = entry.state.changed_at();
            core::mem::replace(
                &mut entry.state,
                WithRevision::new(old_changed_at, NodeEntryState::Pending),
            )
            .into_value()
        };

        match state {
            NodeEntryState::Alive(mut runtime) => {
                let mut ctx = crate::node::DestroyCtx::new(node, frame);
                runtime
                    .destroy(&mut ctx)
                    .map_err(|err| EngineError::node(node, err))?;
            }
            NodeEntryState::Pending | NodeEntryState::Failed { .. } => {}
            NodeEntryState::Executing { call } => {
                let entry = self
                    .tree
                    .get_mut(node)
                    .ok_or(EngineError::UnknownNode(node))?;
                entry.set_state(NodeEntryState::Executing { call: call.clone() }, frame);
                return Err(EngineError::Node {
                    node,
                    message: format!(
                        "cannot remove or reattach node while executing {}",
                        call.call.label()
                    ),
                });
            }
        }

        for buffer_id in self.runtime_buffers.remove_owned_by(node) {
            self.services.unregister_output_sink(buffer_id);
        }
        self.demand_roots.retain(|&root| root != node);
        Ok(())
    }

    pub fn add_binding(
        &mut self,
        draft: BindingDraft,
        revision: Revision,
    ) -> Result<BindingRef, BindingError> {
        let binding_ref = self.tree.add_binding(draft, revision)?;
        // A new binding can win a slot that already resolved, so every
        // cached decision about that slot is now wrong.
        self.resolver.invalidate_structure();
        Ok(binding_ref)
    }

    /// Drop every node-owned binding. The loader's binding phase re-registers
    /// from defs afterwards; see [`crate::node::RuntimeNodeTree::clear_bindings`].
    ///
    /// Goes through the engine rather than the tree so that emptying the
    /// binding graph invalidates resolution on its own, instead of depending
    /// on a later `add_binding` happening to do it.
    pub fn clear_bindings(&mut self, revision: Revision) {
        self.tree.clear_bindings(revision);
        self.resolver.invalidate_structure();
    }

    /// Optional graphics backend for core shader nodes; clone is cheap (`Arc`).
    /// Engage (or update) a panel writer (panel.md P1/P2): latched until
    /// an explicit clear, landing in `scope` at panel priority. Changes
    /// the winning provider set, so cached routes are invalidated.
    pub fn panel_write(
        &mut self,
        scope: crate::node::ScopeRef,
        channel: lpc_model::ChannelName,
        value: lpc_model::LpValue,
        ttl_ms: Option<u32>,
    ) {
        // Momentary liveness (panel.md P14): the deadline is an engine-time
        // instant; renewal is just another write with a fresh TTL.
        let expires_at_ms = ttl_ms.map(|ttl| self.frame_time.total_ms as u64 + u64::from(ttl));
        self.panel_writers
            .set(scope, channel, value, self.revision, expires_at_ms);
        self.resolver.invalidate_structure();
    }

    /// Clear every engaged writer everywhere — sink scopes included
    /// (settled P-Q4). Returns the count.
    pub fn panel_clear_all(&mut self) -> usize {
        let cleared = self.panel_writers.clear_all();
        if cleared > 0 {
            self.resolver.invalidate_structure();
        }
        cleared
    }

    /// Clear one engaged panel writer (panel.md P3). Returns whether
    /// anything was engaged.
    pub fn panel_clear(
        &mut self,
        scope: crate::node::ScopeRef,
        channel: &lpc_model::ChannelName,
    ) -> bool {
        let cleared = self.panel_writers.clear(scope, channel);
        if cleared {
            self.resolver.invalidate_structure();
        }
        cleared
    }

    /// Clear every engaged writer in `scope`; returns the count.
    pub fn panel_clear_scope(&mut self, scope: crate::node::ScopeRef) -> usize {
        let cleared = self.panel_writers.clear_scope(scope);
        if cleared > 0 {
            self.resolver.invalidate_structure();
        }
        cleared
    }

    /// The engaged panel writers (probes, persistence).
    pub fn panel_writers(&self) -> &crate::dataflow::panel_writers::PanelWriterStore {
        &self.panel_writers
    }

    /// The published timebases and their phasors (probes, tests). Runtime
    /// state only — never persisted, never authored.
    pub fn timebases(&self) -> &crate::dataflow::timebase::TimebaseStore {
        &self.timebases
    }

    /// Mutable timebase access, so a test can stand in for the consumer that
    /// P3 will add. Not public: outside the engine, a timebase is read-only
    /// — only a producing node may publish one, and only a tick may advance
    /// a phasor. Gated to the clock+shader feature set its only callers
    /// (`timebase_tests`, `shader_timebase_tests`) compile under, so the
    /// gate-off builds of `check-lpc-engine-gates` stay warning-free.
    #[cfg(all(test, feature = "node-clock", feature = "node-shader"))]
    pub(crate) fn timebases_mut(&mut self) -> &mut crate::dataflow::timebase::TimebaseStore {
        &mut self.timebases
    }

    pub fn set_graphics(&mut self, graphics: Option<Arc<dyn LpGraphics>>) {
        self.graphics = graphics;
    }

    /// Set (or clear) the device-level safe-mode output ceiling.
    ///
    /// `level` is a brightness ceiling out of 255, from the boot-control
    /// record's clamp bits. Applies to EVERY fixture — including ones with
    /// no power model — because the clamped project may predate the power
    /// feature entirely.
    pub fn set_safe_output_clamp(&mut self, level: Option<u8>) {
        self.safe_output_clamp_q16 = level.map(|level| (u32::from(level) << 16) / 255);
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
        // The runtime is the authority on its own status from the moment it
        // attaches, not only after some later change: a placeholder
        // standing in for a gated-out node kind must report `Unsupported`
        // on the FIRST tree delta the client sees, or the node arrives
        // looking like a healthy `Created` one. Runtimes that report
        // nothing (`runtime_status() == None`, the trait default) leave the
        // entry's status exactly as it was.
        let attached_status = runtime.runtime_status();
        let entry = self.tree.get_mut(id).ok_or(EngineError::UnknownNode(id))?;
        if let Some(status) = attached_status {
            set_entry_status_if_changed(entry, status, frame);
        }
        entry.set_state(NodeEntryState::Alive(runtime), frame);
        Ok(())
    }

    /// Dispatch a runtime node command (`WireProjectCommand::NodeCommand`)
    /// to the addressed node's live runtime.
    ///
    /// The runtime command channel is the non-overlay client→engine write
    /// path (`docs/adr/2026-07-27-runtime-node-command-channel.md`): the
    /// command acts on live runtime state only — no staging, no
    /// persistence, no revision bump here (effects surface through the
    /// node's own runtime slots on the next produce). Unknown/not-alive
    /// nodes and runtime-refused commands come back as errors for the
    /// server to answer as a normal `Rejected` response.
    pub fn handle_node_command(
        &mut self,
        node: NodeId,
        command: &lpc_wire::WireNodeCommand,
    ) -> Result<(), EngineError> {
        let time_s = self.frame_time.total_ms as f32 / 1000.0;
        let entry = self
            .tree
            .get_mut(node)
            .ok_or(EngineError::UnknownNode(node))?;
        match entry.state.get_mut() {
            NodeEntryState::Alive(runtime) => runtime
                .handle_command(command, time_s)
                .map_err(|e| EngineError::node(node, e)),
            _ => Err(EngineError::NotAlive(node)),
        }
    }

    pub fn runtime_output_sink_buffer_id(&self, node_id: NodeId) -> Option<RuntimeBufferId> {
        let entry = self.tree.get(node_id)?;
        match entry.state.value() {
            NodeEntryState::Alive(node) => node.runtime_output_sink_buffer_id(),
            _ => None,
        }
    }

    // Consumed by texture and shader node test modules (and the
    // texture-def-root test in `project_read_stream`).
    #[cfg(all(test, any(feature = "node-texture", feature = "node-shader")))]
    pub(crate) fn load_test_node_defs(
        &mut self,
        registry: &mut ProjectRegistry,
        defs: &[(NodeId, NodeDef)],
        frame: Revision,
    ) -> Result<(), alloc::string::String> {
        use alloc::format;
        use alloc::string::String;
        use lpc_model::{ArtifactLocation, NodeDefLocation};
        use lpc_registry::ParseCtx;
        use lpfs::lp_path::AsLpPath;
        use lpfs::{LpFs, LpFsMemory};

        let fs = LpFsMemory::new();
        let mut node_lines = String::new();
        for (index, (_, def)) in defs.iter().enumerate() {
            let node_path = format!("/test-node-{index}.json");
            if index > 0 {
                node_lines.push(',');
            }
            node_lines.push_str(&format!("\"node{index}\": {{ \"ref\": \".{node_path}\" }}"));
            let text = def
                .write_json(&self.slot_shapes)
                .map_err(|e| e.to_string())?;
            fs.write_file(node_path.as_path(), text.as_bytes())
                .map_err(|e| e.to_string())?;
        }
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 8\n}\n")
            .map_err(|e| e.to_string())?;
        let module = format!("{{ \"kind\": \"Module\", \"nodes\": {{ {node_lines} }} }}");
        fs.write_file("/module.json".as_path(), module.as_bytes())
            .map_err(|e| e.to_string())?;

        let ctx = ParseCtx {
            shapes: &self.slot_shapes,
        };
        registry
            .load_root(&fs, "/module.json".as_path(), frame, &ctx)
            .map_err(|e| format!("{e:?}"))?;

        for (index, (node_id, _)) in defs.iter().enumerate() {
            let location = NodeDefLocation::artifact_root(ArtifactLocation::file(format!(
                "/test-node-{index}.json"
            )));
            let entry = self
                .tree
                .get_mut(*node_id)
                .ok_or_else(|| format!("unknown test node {node_id:?}"))?;
            entry.def_location = Some(location);
        }

        Ok(())
    }

    pub fn tick(&mut self, registry: &ProjectRegistry, delta_ms: u32) -> Result<(), EngineError> {
        lp_perf::emit_begin!(lp_perf::EVENT_FRAME);
        let result = (|| {
            self.tick_nodes(registry, delta_ms)?;
            let revision = self.revision;
            self.refresh_output_sink_configs(registry);
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

    /// Broadcast memory pressure to every alive node.
    ///
    /// ⚠️ Only call at a safe point — a moment where no render borrow into any
    /// node-owned buffer is live. The engine calls this at the top of a tick
    /// when a compile window was requested; embedders may also call it between
    /// ticks (e.g. from an allocation-failure retry hook, OUTSIDE the
    /// allocator lock). Anything a node drops here must be rebuilt lazily on
    /// its next render — that is the contract in
    /// `docs/adr/2026-08-03-memory-pressure-at-compile-safe-points.md`.
    pub fn broadcast_memory_pressure(
        &mut self,
        level: crate::node::PressureLevel,
    ) -> Result<(), EngineError> {
        let revision = self.revision;
        for entry in self.tree.entries_mut() {
            let node_id = entry.id;
            if let NodeEntryState::Alive(node) = entry.state.get_mut() {
                let mut ctx = crate::node::MemPressureCtx::new(node_id, revision);
                node.handle_memory_pressure(level, &mut ctx)
                    .map_err(|err| EngineError::node(node_id, err))?;
            }
        }
        Ok(())
    }

    /// Re-read every output's authored configuration for this tick.
    ///
    /// The tree and the services are borrowed as separate fields so the defs
    /// can be read while the sinks are updated. Collecting the work first
    /// instead cost a full clone of every output's definition — endpoint spec
    /// and all — on every frame, to almost always discover that nothing had
    /// changed.
    fn refresh_output_sink_configs(&mut self, registry: &ProjectRegistry) {
        let tree = &self.tree;
        let services = &mut self.services;
        for entry in tree.entries() {
            let buffer_id = match entry.state.value() {
                NodeEntryState::Alive(node) => node.runtime_output_sink_buffer_id(),
                _ => None,
            };
            let Some(buffer_id) = buffer_id else {
                continue;
            };
            let Some(location) = entry.def_location.as_ref() else {
                continue;
            };
            let Ok(NodeDef::Output(def)) = loaded_registry_def(registry, location) else {
                continue;
            };
            services.update_output_sink_config(buffer_id, entry.id, def);
        }
    }

    fn tick_nodes(&mut self, registry: &ProjectRegistry, delta_ms: u32) -> Result<(), EngineError> {
        #[cfg(debug_assertions)]
        self.assert_structural_changes_were_announced();
        self.resolver.begin_frame();
        self.frame_num = self.frame_num.next();
        self.revision = advance_revision();
        self.frame_time =
            FrameTime::new(delta_ms, self.frame_time.total_ms.saturating_add(delta_ms));
        // Momentary panel writers despawn on expiry (panel.md P14) — the
        // despawn IS the release fallback for a dropped client.
        if self
            .panel_writers
            .despawn_expired(self.frame_time.total_ms as u64)
            > 0
        {
            self.resolver.invalidate_structure();
        }

        // Compile window (memory-pressure seam). A shader node that wanted a
        // compile last frame deferred it and requested a window. The top of a
        // tick is a safe point — no render borrow into any per-LED buffer is
        // live — so drop rebuildable state across the whole tree NOW, then
        // open this frame's window. Demand order inside the tick guarantees
        // the compile runs before the fixture seams rebuild what was dropped:
        // the fixture resolves its visual input (where the shader compiles)
        // before `ensure_direct_points`/sample-buffer allocation. See
        // docs/adr/2026-08-03-memory-pressure-at-compile-safe-points.md.
        let window_wanted = self.tree.entries().any(|entry| {
            matches!(entry.state.value(), NodeEntryState::Alive(node) if node.wants_compile_window())
        });
        if window_wanted {
            self.broadcast_memory_pressure(crate::node::PressureLevel::High)?;
            let revision = self.revision;
            for entry in self.tree.entries_mut() {
                if let NodeEntryState::Alive(node) = entry.state.get_mut() {
                    node.open_compile_window(revision);
                }
            }
        }

        let mut resolver = core::mem::replace(&mut self.resolver, Resolver::new());
        let trace = ResolveTrace::new(ResolveLogLevel::Off);
        let mut session = EngineSession::new(self.revision, &mut resolver, trace);

        let mut producers_ticked = VecSet::new();
        let time_s = self.frame_time.total_ms as f32 / 1000.0;
        let time_provider = self.services.time_provider();
        let button_service = self.services.button_service();
        let radio_service = self.services.radio_service();
        let mut host = EngineResolveHost {
            tree: &mut self.tree,
            registry,
            panel_writers: &self.panel_writers,
            timebases: &mut self.timebases,
            producers_ticked: &mut producers_ticked,
            runtime_buffers: &mut self.runtime_buffers,
            slot_shapes: &self.slot_shapes,
            graphics: self.graphics.clone(),
            time_provider,
            button_service,
            radio_service,
            frame_time_seconds: time_s,
            safe_output_clamp_q16: self.safe_output_clamp_q16,
            frame_revision: self.revision,
        };

        let walk = (|| {
            for &root in &self.demand_roots {
                consume_tree_node(&mut session, &mut host, root)?;
            }
            Ok(())
        })();

        // End-of-tick timebase maintenance, deliberately OUTSIDE the `?`
        // path above: a node erroring mid-walk must not stall the store's
        // tick, or every phasor in the project would silently freeze until
        // the offending node was fixed.
        let tree = &self.tree;
        self.timebases.sweep(|clock| {
            matches!(
                tree.get(clock).map(|entry| entry.state.value()),
                Some(NodeEntryState::Alive(_))
            )
        });

        self.resolver = resolver;
        walk
    }

    /// Fail loudly when the graph changed shape without anyone calling
    /// [`Resolver::invalidate_structure`].
    ///
    /// The invalidation contract is the load-bearing rule behind persisting
    /// resolution across frames, and breaking it is silent by nature: the
    /// resolver keeps serving an answer that is stale but entirely plausible.
    /// Prose in an ADR does not survive contact with a new mutation site, so
    /// the rule checks itself.
    ///
    /// Debug builds only — release firmware pays nothing. That is the right
    /// trade for a guard whose whole job is to fire during development and
    /// tests, long before a device is involved.
    #[cfg(debug_assertions)]
    fn assert_structural_changes_were_announced(&mut self) {
        let fingerprint = self.tree.structural_fingerprint();
        let epoch = self.resolver.structure_epoch();
        if let Some((previous_fingerprint, previous_epoch)) = self.last_structural_check {
            debug_assert!(
                fingerprint == previous_fingerprint || epoch != previous_epoch,
                "the node tree changed shape ({previous_fingerprint:?} -> {fingerprint:?}) \
                 without Resolver::invalidate_structure(); resolution cached against the old \
                 graph is now being served. Whatever mutated the tree or its bindings must \
                 announce it — see docs/adr/2026-07-31-resolver-persistent-resolution.md"
            );
        }
        self.last_structural_check = Some((fingerprint, epoch));
    }

    /// Materialize a visual product handle into a CPU texture.
    ///
    /// This is the same materialization the wire render-product probe uses;
    /// it is public so host-side preview surfaces (e.g. the browser preview
    /// lab) can pull frames without routing pixels through the JSON protocol.
    pub fn render_texture_product(
        &mut self,
        registry: &ProjectRegistry,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, SessionResolveError> {
        let mut producers_ticked = VecSet::new();
        let time_s = self.frame_time.total_ms as f32 / 1000.0;
        let time_provider = self.services.time_provider();
        let button_service = self.services.button_service();
        let radio_service = self.services.radio_service();
        let mut host = EngineResolveHost {
            tree: &mut self.tree,
            registry,
            panel_writers: &self.panel_writers,
            timebases: &mut self.timebases,
            producers_ticked: &mut producers_ticked,
            runtime_buffers: &mut self.runtime_buffers,
            slot_shapes: &self.slot_shapes,
            graphics: self.graphics.clone(),
            time_provider,
            button_service,
            radio_service,
            frame_time_seconds: time_s,
            safe_output_clamp_q16: self.safe_output_clamp_q16,
            frame_revision: self.revision,
        };
        host.render_node_texture(product, request)
    }

    /// Ask a visual product's producer what space it renders in (plan D17),
    /// without materializing a frame.
    ///
    /// This is the same node-to-node query the fixture negotiation path
    /// issues every render (`ControlRenderServices::visual_product_space`);
    /// public here so preview surfaces can introspect a producer's
    /// primary space and 2D answer — e.g. to recompute
    /// [`crate::products::visual::resolve_1d_to_2d_with_origin`] for a
    /// caption — without threading that origin through the render call
    /// itself.
    pub fn visual_product_space(
        &mut self,
        registry: &ProjectRegistry,
        product: VisualProduct,
    ) -> Result<ProductSpaceInfo, SessionResolveError> {
        let mut producers_ticked = VecSet::new();
        let time_s = self.frame_time.total_ms as f32 / 1000.0;
        let time_provider = self.services.time_provider();
        let button_service = self.services.button_service();
        let radio_service = self.services.radio_service();
        let mut host = EngineResolveHost {
            tree: &mut self.tree,
            registry,
            panel_writers: &self.panel_writers,
            timebases: &mut self.timebases,
            producers_ticked: &mut producers_ticked,
            runtime_buffers: &mut self.runtime_buffers,
            slot_shapes: &self.slot_shapes,
            graphics: self.graphics.clone(),
            time_provider,
            button_service,
            radio_service,
            frame_time_seconds: time_s,
            safe_output_clamp_q16: self.safe_output_clamp_q16,
            frame_revision: self.revision,
        };
        host.visual_node_space(product)
    }

    /// Resolve a bus channel to the visual product handle it currently carries.
    ///
    /// Preview surfaces call this once after a project loads to discover the
    /// product behind a visual bus channel (conventionally `visual.out`), then
    /// materialize frames with [`Self::render_texture_product`]. Product
    /// handles are node-owned and stay stable across frames, so callers should
    /// cache the result rather than re-resolving per frame (resolution may
    /// re-run producer nodes).
    pub fn resolve_bus_visual_product(
        &mut self,
        registry: &ProjectRegistry,
        channel: &str,
    ) -> Result<VisualProduct, SessionResolveError> {
        match self.resolve_bus_product(registry, channel)? {
            lpc_model::ProductRef::Visual(product) => Ok(product),
            other => Err(SessionResolveError::other(format!(
                "bus channel {channel:?} does not carry a visual product (got {other:?})"
            ))),
        }
    }

    /// Resolve a bus channel to the control product handle it currently
    /// carries — the control sibling of [`Self::resolve_bus_visual_product`].
    ///
    /// This is the engine-side answer to "is this project control-first?": a
    /// root scope whose `control.out` resolves to a control product is
    /// driving lamps, whatever its visual side does. Callers ask once after a
    /// project loads and cache the answer (the resolve may re-run producer
    /// nodes); an `Err` is the honest "nothing publishes control here",
    /// not a fault.
    pub fn resolve_bus_control_product(
        &mut self,
        registry: &ProjectRegistry,
        channel: &str,
    ) -> Result<ControlProduct, SessionResolveError> {
        match self.resolve_bus_product(registry, channel)? {
            lpc_model::ProductRef::Control(product) => Ok(product),
            other => Err(SessionResolveError::other(format!(
                "bus channel {channel:?} does not carry a control product (got {other:?})"
            ))),
        }
    }

    /// Resolve the root scope's `channel` to whatever product it carries.
    fn resolve_bus_product(
        &mut self,
        registry: &ProjectRegistry,
        channel: &str,
    ) -> Result<lpc_model::ProductRef, SessionResolveError> {
        let key = QueryKey::Bus {
            scope: self.tree.node_scope(self.tree.root()),
            channel: lpc_model::ChannelName(channel.to_string()),
        };
        let fid = self.revision;
        let mut resolver_tmp = core::mem::replace(&mut self.resolver, Resolver::new());
        // A forced-fresh read, not an invalidation: the caller wants values
        // re-resolved rather than whatever the last tick left behind. The
        // graph has not changed, so structural knowledge must survive.
        resolver_tmp.begin_frame();
        let mut session = EngineSession::new(
            fid,
            &mut resolver_tmp,
            ResolveTrace::new(ResolveLogLevel::Off),
        );
        let mut producers_ticked = VecSet::new();
        let time_s = self.frame_time.total_ms as f32 / 1000.0;
        let time_provider = self.services.time_provider();
        let button_service = self.services.button_service();
        let radio_service = self.services.radio_service();
        let mut host = EngineResolveHost {
            tree: &mut self.tree,
            registry,
            panel_writers: &self.panel_writers,
            timebases: &mut self.timebases,
            producers_ticked: &mut producers_ticked,
            runtime_buffers: &mut self.runtime_buffers,
            slot_shapes: &self.slot_shapes,
            graphics: self.graphics.clone(),
            time_provider,
            button_service,
            radio_service,
            frame_time_seconds: time_s,
            safe_output_clamp_q16: self.safe_output_clamp_q16,
            frame_revision: self.revision,
        };
        let result = session.resolve(&mut host, &key);
        self.resolver = resolver_tmp;
        let production = result?;
        let Some(leaf) = production.value_leaf() else {
            return Err(SessionResolveError::other(format!(
                "bus channel {channel:?} did not resolve to a leaf value"
            )));
        };
        match leaf.value() {
            lpc_model::LpValue::Product(product) => Ok(*product),
            other => Err(SessionResolveError::other(format!(
                "bus channel {channel:?} does not carry a product (got {other:?})"
            ))),
        }
    }

    #[cfg(test)]
    pub(crate) fn render_texture_for_test(
        &mut self,
        registry: &ProjectRegistry,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, SessionResolveError> {
        self.render_texture_product(registry, product, request)
    }

    // Consumed only by fixture node control-rendering tests.
    #[cfg(all(test, feature = "node-fixture"))]
    pub(crate) fn render_control_for_test(
        &mut self,
        registry: &ProjectRegistry,
        product: ControlProduct,
        request: &ControlRenderRequest,
        target: ControlRenderTarget<'_>,
    ) -> Result<ControlLayout, SessionResolveError> {
        let mut producers_ticked = VecSet::new();
        let time_s = self.frame_time.total_ms as f32 / 1000.0;
        let time_provider = self.services.time_provider();
        let button_service = self.services.button_service();
        let radio_service = self.services.radio_service();
        let mut host = EngineResolveHost {
            tree: &mut self.tree,
            registry,
            panel_writers: &self.panel_writers,
            timebases: &mut self.timebases,
            producers_ticked: &mut producers_ticked,
            runtime_buffers: &mut self.runtime_buffers,
            slot_shapes: &self.slot_shapes,
            graphics: self.graphics.clone(),
            time_provider,
            button_service,
            radio_service,
            frame_time_seconds: time_s,
            safe_output_clamp_q16: self.safe_output_clamp_q16,
            frame_revision: self.revision,
        };
        host.render_node_control(product, request, target)
    }

    /// Resolve a bus channel's current value on demand, outside the tick.
    ///
    /// Reuses the current frame's resolver cache so probe reads see the same
    /// values the last tick produced (values already demanded this frame are
    /// free; undemanded channels resolve fresh, like render probes do).
    pub(crate) fn resolve_bus_channel_value(
        &mut self,
        registry: &ProjectRegistry,
        scope: Option<crate::node::ScopeRef>,
        channel: &ChannelName,
    ) -> Result<Production, SessionResolveError> {
        let mut resolver = core::mem::replace(&mut self.resolver, Resolver::new());
        let mut session = EngineSession::new(
            self.revision,
            &mut resolver,
            ResolveTrace::new(ResolveLogLevel::Off),
        );
        let mut producers_ticked = VecSet::new();
        let time_s = self.frame_time.total_ms as f32 / 1000.0;
        let time_provider = self.services.time_provider();
        let button_service = self.services.button_service();
        let radio_service = self.services.radio_service();
        let mut host = EngineResolveHost {
            tree: &mut self.tree,
            registry,
            panel_writers: &self.panel_writers,
            timebases: &mut self.timebases,
            producers_ticked: &mut producers_ticked,
            runtime_buffers: &mut self.runtime_buffers,
            slot_shapes: &self.slot_shapes,
            graphics: self.graphics.clone(),
            time_provider,
            button_service,
            radio_service,
            frame_time_seconds: time_s,
            safe_output_clamp_q16: self.safe_output_clamp_q16,
            frame_revision: self.revision,
        };
        let scope = scope.or_else(|| host.tree.node_scope(host.tree.root()));
        let result = session.resolve(
            &mut host,
            &QueryKey::Bus {
                scope,
                channel: channel.clone(),
            },
        );
        self.resolver = resolver;
        result
    }

    pub(crate) fn render_control_product_probe(
        &mut self,
        registry: &ProjectRegistry,
        product: ControlProduct,
        request: &ControlRenderRequest,
        target: ControlRenderTarget<'_>,
        display_layout: ControlDisplayLayoutRead,
    ) -> Result<(ControlLayout, ControlDisplayLayoutProbeResult), SessionResolveError> {
        let mut producers_ticked = VecSet::new();
        let time_s = self.frame_time.total_ms as f32 / 1000.0;
        let time_provider = self.services.time_provider();
        let button_service = self.services.button_service();
        let radio_service = self.services.radio_service();
        let mut host = EngineResolveHost {
            tree: &mut self.tree,
            registry,
            panel_writers: &self.panel_writers,
            timebases: &mut self.timebases,
            producers_ticked: &mut producers_ticked,
            runtime_buffers: &mut self.runtime_buffers,
            slot_shapes: &self.slot_shapes,
            graphics: self.graphics.clone(),
            time_provider,
            button_service,
            radio_service,
            frame_time_seconds: time_s,
            safe_output_clamp_q16: self.safe_output_clamp_q16,
            frame_revision: self.revision,
        };
        host.render_node_control_probe(product, request, target, display_layout)
    }

    /// Ask a control producer for its display layout WITHOUT rendering it.
    ///
    /// The geometry half of [`Self::render_control_product_probe`], split out
    /// for the published-frame read: that read already has the samples (the
    /// output node published them last tick) and needs only the lamp
    /// positions to draw them. Producers answer from cached mapping state, so
    /// the cost is O(lamps) with no graph resolve and no shader work — which
    /// is the whole point of not going through the control-product probe.
    pub(crate) fn control_display_layout_probe(
        &mut self,
        registry: &ProjectRegistry,
        product: ControlProduct,
        display_layout: ControlDisplayLayoutRead,
    ) -> Result<ControlDisplayLayoutProbeResult, SessionResolveError> {
        let mut producers_ticked = VecSet::new();
        let time_s = self.frame_time.total_ms as f32 / 1000.0;
        let time_provider = self.services.time_provider();
        let button_service = self.services.button_service();
        let radio_service = self.services.radio_service();
        let mut host = EngineResolveHost {
            tree: &mut self.tree,
            registry,
            panel_writers: &self.panel_writers,
            timebases: &mut self.timebases,
            producers_ticked: &mut producers_ticked,
            runtime_buffers: &mut self.runtime_buffers,
            slot_shapes: &self.slot_shapes,
            graphics: self.graphics.clone(),
            time_provider,
            button_service,
            radio_service,
            frame_time_seconds: time_s,
            safe_output_clamp_q16: self.safe_output_clamp_q16,
            frame_revision: self.revision,
        };
        host.node_control_display_layout(product, display_layout)
    }
}

/// Host adapter with borrows disjoint from the [`Resolver`] handed to [`EngineSession`].
/// The synthetic provider an engaged panel writer contributes: a literal
/// source at panel priority, owned by the scope's introducing node. The
/// binding ref uses a sentinel index — panel writers live in the side
/// store, never in any node's binding set.
fn panel_provider(
    tree: &RuntimeNodeTree<Box<dyn NodeRuntime>>,
    scope: crate::node::ScopeRef,
    channel: &lpc_model::ChannelName,
    writer: &crate::dataflow::panel_writers::PanelWriter,
) -> (BindingRef, crate::dataflow::binding::BindingEntry) {
    let kind = tree
        .bus_channels()
        .find(|(name, _)| *name == channel)
        .map(|(_, kind)| kind)
        .unwrap_or(lpc_model::Kind::Ratio);
    (
        BindingRef::new(scope.owner(), usize::MAX),
        crate::dataflow::binding::BindingEntry {
            source: crate::dataflow::binding::BindingSource::Literal(writer.value.clone()),
            target: crate::dataflow::binding::BindingTarget::BusChannel(channel.clone()),
            priority: crate::dataflow::binding::BindingPriority::panel(),
            kind,
            version: writer.written_at,
            owner: scope.owner(),
        },
    )
}

struct EngineResolveHost<'a> {
    tree: &'a mut RuntimeNodeTree<Box<dyn NodeRuntime>>,
    registry: &'a ProjectRegistry,
    panel_writers: &'a crate::dataflow::panel_writers::PanelWriterStore,
    timebases: &'a mut crate::dataflow::timebase::TimebaseStore,
    producers_ticked: &'a mut VecSet<NodeId>,
    runtime_buffers: &'a mut RuntimeBufferStore,
    slot_shapes: &'a SlotShapeRegistry,
    graphics: Option<Arc<dyn LpGraphics>>,
    time_provider: Option<Rc<dyn TimeProvider>>,
    button_service: Option<Rc<dyn ButtonService>>,
    radio_service: Option<Rc<dyn RadioService>>,
    frame_time_seconds: f32,
    safe_output_clamp_q16: Option<u32>,
    /// The engine's current frame revision — the same value the tick stamps
    /// on compile windows ([`NodeRuntime::open_compile_window`]).
    ///
    /// Render contexts must carry THIS, not the ambient
    /// [`lpc_model::current_revision`]: the ambient counter is
    /// process-global, so anything else advancing it between the tick's
    /// `advance_revision` and a node's render (a second engine in the
    /// process, or parallel tests sharing the binary) desyncs the two and
    /// the node sees a window that never matches its render frame. That is
    /// exactly how a shader deferred its compile forever — see
    /// `docs/defects/2026-08-03-render-context-revision-read-from-ambient-counter.md`.
    frame_revision: Revision,
}

impl EngineResolveHost<'_> {
    #[inline(never)]
    fn produce_produced_slot(
        &mut self,
        node: NodeId,
        slot: &SlotPath,
        session: &mut EngineSession<'_>,
    ) -> Result<Production, SessionResolveError> {
        self.produce_node_slot(node, slot, session)?;
        let entry = self.tree.get(node).ok_or_else(|| {
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
            SessionResolveError::other(format!("missing produced slot {slot:?} on {node:?}: {e}"))
        })?;
        Ok(Production::new(
            product,
            ProductionSource::ProducedSlot {
                node,
                slot: slot.clone(),
            },
        ))
    }

    #[inline(never)]
    fn produce_consumed_slot(
        &self,
        node: NodeId,
        slot: &SlotPath,
    ) -> Result<Production, SessionResolveError> {
        let _entry =
            self.tree
                .get(node)
                .ok_or_else(|| SessionResolveError::UnresolvedConsumedSlot {
                    node,
                    slot: slot.clone(),
                })?;
        // A shader uniform is not a field of `ShaderDef` — it lives under
        // `consumed[<name>]` — so the plain def lookup below can never see
        // it. Project it first, the way the merge-policy read does.
        if let Ok(Some(product)) = self.read_shader_consumed_slot_default(node, slot) {
            return Ok(Production::new(product, ProductionSource::Default));
        }

        let product = self.read_authored_def_product(node, slot).map_err(|_| {
            SessionResolveError::UnresolvedConsumedSlot {
                node,
                slot: slot.clone(),
            }
        })?;
        Ok(Production::new(product, ProductionSource::Default))
    }

    #[inline(never)]
    fn produce_consumed_slot_accessor(
        &self,
        node: NodeId,
        accessor: &SlotAccessor,
    ) -> Result<Production, SessionResolveError> {
        let _entry =
            self.tree
                .get(node)
                .ok_or_else(|| SessionResolveError::UnresolvedConsumedSlot {
                    node,
                    slot: accessor.path().clone(),
                })?;
        let product = self
            .read_authored_def_product_by_accessor(node, accessor)
            .map_err(|_| SessionResolveError::UnresolvedConsumedSlot {
                node,
                slot: accessor.path().clone(),
            })?;
        Ok(Production::new(product, ProductionSource::Default))
    }

    fn produce_node_slot(
        &mut self,
        node_id: NodeId,
        slot: &SlotPath,
        session: &mut EngineSession<'_>,
    ) -> Result<(), SessionResolveError> {
        if self.producers_ticked.contains(&node_id) {
            return Ok(());
        }

        let revision = session.revision();
        let restore_frame = session.revision();
        let mut node_runtime = {
            let entry = self.tree.get_mut(node_id).ok_or_else(|| {
                SessionResolveError::other(format!("produce: unknown node {node_id:?}"))
            })?;
            let old_changed_at = entry.state.changed_at();
            let executing = NodeEntryState::Executing {
                call: NodeCallKey::new(node_id, NodeCall::ProduceSlot { slot: slot.clone() }),
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
            node_runtime
        };

        let gfx = self.graphics.clone();
        let time_provider = self.time_provider.clone();
        let button_service = self.button_service.clone();
        let radio_service = self.radio_service.clone();
        let time_s = self.frame_time_seconds;
        let slot_shapes = self.slot_shapes;
        let recovery_name = recovery_frame_name(&self.tree, node_id);
        let produce_result = {
            let mut bridge = SessionHostResolver {
                session,
                host: self as &mut dyn ResolveHost,
            };
            let resolver_dyn: &mut dyn TickResolver = &mut bridge;
            let mut tick_ctx = TickContext::with_engine_services(
                node_id,
                revision,
                resolver_dyn,
                slot_shapes,
                gfx,
                time_provider,
                button_service,
                radio_service,
                time_s,
            );
            catch_node_panic_framed(lp_recovery::FrameKind::NodeRender, &recovery_name, || {
                node_runtime.produce(slot, &mut tick_ctx)
            })
        };

        let entry = self.tree.get_mut(node_id).ok_or_else(|| {
            SessionResolveError::other(format!("produce: unknown node {node_id:?}"))
        })?;
        let runtime_status = runtime_status_or_ok(&*node_runtime);
        entry.set_state(NodeEntryState::Alive(node_runtime), restore_frame);

        match produce_result {
            Ok(ProduceResult::Produced) => {
                set_entry_status_if_changed(entry, runtime_status, revision);
                self.producers_ticked.insert(node_id);
                Ok(())
            }
            // A gated-out kind produces nothing, so EVERY such node lands
            // here — and "does not produce slot" reads as an authoring
            // mistake when the real cause is the build. The node's own
            // status already names that cause; use it, and leave genuine
            // wrong-slot diagnostics on real nodes untouched.
            Ok(ProduceResult::Unsupported) => {
                let message = match &runtime_status {
                    NodeRuntimeStatus::Unsupported(cause) => format!(
                        "produce: {cause} (node {node_id:?} is a placeholder for slot {slot:?})"
                    ),
                    _ => format!("produce: node {node_id:?} does not produce slot {slot:?}"),
                };
                set_entry_status_if_changed(entry, runtime_status, revision);
                Err(SessionResolveError::other(message))
            }
            Err(e) => {
                let message = e.to_string();
                set_entry_status_if_changed(
                    entry,
                    NodeRuntimeStatus::Error(message.clone()),
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
                self.produce_produced_slot(*node, slot, session)
            }
            QueryKey::ConsumedSlot { node, slot } => self.produce_consumed_slot(*node, slot),
            QueryKey::ConsumedSlotAccessor { node, accessor } => {
                self.produce_consumed_slot_accessor(*node, accessor)
            }
            QueryKey::Bus { .. } => Err(SessionResolveError::other(
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

    /// The R5 shadowing walk again, but reporting WHERE it stopped rather
    /// than what it found there: a phasor's shared identity is the scope the
    /// winning writer lives in, so two readers in different scopes that both
    /// resolve outward to the same writer ride the same integrator.
    ///
    /// Deliberately not "the winning provider's `node_scope`" — a panel
    /// writer's synthetic provider is owned by the scope's module node, which
    /// *inhabits its parent scope*, and that owner-based answer would name
    /// the wrong scope for exactly the case a shared knob is for.
    fn consumed_slot_bus_provenance(
        &self,
        node: NodeId,
        slot: &SlotPath,
    ) -> Option<(crate::node::ScopeRef, lpc_model::ChannelName)> {
        let (_, entry) = self.tree.binding_for_consumed_slot(node, slot)?;
        let crate::dataflow::binding::BindingSource::BusChannel(channel) = &entry.source else {
            return None;
        };
        let channel = channel.clone();
        let mut scope = self.node_scope(node)?;
        loop {
            if self.panel_writers.get(scope, &channel).is_some()
                || !self
                    .tree
                    .providers_for_bus_in_scope(scope, &channel)
                    .is_empty()
            {
                return Some((scope, channel));
            }
            scope = self.tree.parent_scope(scope)?;
        }
    }

    fn node_scope(&self, node: NodeId) -> Option<crate::node::ScopeRef> {
        // Reading scope (R7 export semantics): introducers read inward,
        // everyone else reads the scope they inhabit. Write-side provider
        // classification stays on `NodeTree::node_scope` (R4).
        self.tree.bus_read_scope(node)
    }

    fn providers_for_bus(
        &self,
        scope: Option<crate::node::ScopeRef>,
        channel: &lpc_model::ChannelName,
    ) -> Vec<(BindingRef, crate::dataflow::binding::BindingEntry)> {
        // The R5 walk with the panel overlay (panel.md P4): at each scope,
        // an engaged panel writer REPLACES the scope's provider set —
        // including for ByKey merge channels, where max priority wins
        // rather than merging — and an engaged scope counts as "has a
        // writer" for shadowing even when nothing authored writes there.
        let Some(mut scope) = scope else {
            return self
                .tree
                .providers_for_bus_read(None, channel)
                .into_iter()
                .map(|(binding_ref, entry)| (binding_ref, entry.clone()))
                .collect();
        };
        loop {
            if let Some(writer) = self.panel_writers.get(scope, channel) {
                return alloc::vec![panel_provider(self.tree, scope, channel, writer)];
            }
            let candidates: Vec<(BindingRef, crate::dataflow::binding::BindingEntry)> = self
                .tree
                .providers_for_bus_in_scope(scope, channel)
                .into_iter()
                .map(|(binding_ref, entry)| (binding_ref, entry.clone()))
                .collect();
            if !candidates.is_empty() {
                return candidates;
            }
            match self.tree.parent_scope(scope) {
                Some(parent) => scope = parent,
                None => return Vec::new(),
            }
        }
    }

    fn merge_policy_for_consumed_slot(&self, node: NodeId, slot: &SlotPath) -> SlotMerge {
        let Some(_entry) = self.tree.get(node) else {
            return SlotMerge::Latest;
        };
        if let Ok(Some(policy)) = self.read_shader_consumed_slot_merge_policy(node, slot) {
            return policy;
        }
        self.read_authored_def_slot_semantics(node, slot)
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

    fn publish_timebase(
        &mut self,
        clock: NodeId,
        effective_seconds: f32,
        delta_seconds: f32,
        at: Revision,
    ) {
        self.timebases
            .set_timebase(clock, effective_seconds, delta_seconds, at);
    }

    fn time_product_seconds(
        &self,
        product: lpc_model::TimeProduct,
    ) -> Result<f32, SessionResolveError> {
        self.timebases
            .seconds(product.node())
            .ok_or_else(|| unpublished_timebase(product))
    }

    fn time_product_delta(
        &self,
        product: lpc_model::TimeProduct,
    ) -> Result<f32, SessionResolveError> {
        self.timebases
            .delta(product.node())
            .ok_or_else(|| unpublished_timebase(product))
    }

    fn time_product_phasor(
        &mut self,
        product: lpc_model::TimeProduct,
        key: &crate::dataflow::timebase::PhasorKey,
        config: &lpc_model::PhasorConfig,
        reader: (NodeId, &lpc_model::SlotPath),
    ) -> Result<(f32, u32), SessionResolveError> {
        self.timebases
            .phasor_tick(product.node(), key, config, reader)
            .ok_or_else(|| unpublished_timebase(product))
    }
}

/// A handle whose producer has not published a timebase this run.
///
/// Loud on purpose: the handle names a node, so a miss means the graph is
/// wired to something that is not producing time — not that time is zero.
fn unpublished_timebase(product: lpc_model::TimeProduct) -> SessionResolveError {
    SessionResolveError::other(format!(
        "node {:?} has published no timebase for time product output {}",
        product.node(),
        product.output()
    ))
}

impl crate::node::TimebaseRead for EngineResolveHost<'_> {
    fn time_product_seconds(&self, product: lpc_model::TimeProduct) -> Result<f32, NodeError> {
        self.timebases
            .seconds(product.node())
            .ok_or_else(|| NodeError::msg(format!("{}", unpublished_timebase(product))))
    }

    fn time_product_delta(&self, product: lpc_model::TimeProduct) -> Result<f32, NodeError> {
        self.timebases
            .delta(product.node())
            .ok_or_else(|| NodeError::msg(format!("{}", unpublished_timebase(product))))
    }

    fn time_product_phasor_read(
        &self,
        product: lpc_model::TimeProduct,
        key: &crate::dataflow::timebase::PhasorKey,
    ) -> Result<(f32, u32), NodeError> {
        self.timebases
            .phasor_read(product.node(), key)
            .ok_or_else(|| NodeError::msg(format!("{}", unpublished_timebase(product))))
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
        node: NodeId,
        slot: &SlotPath,
    ) -> Result<SlotData, SessionResolveError> {
        let def = self.loaded_node_def(node)?;
        let (data, shape) = lookup_slot_data_and_shape(def, self.slot_shapes, slot)
            .map_err(|e| SessionResolveError::other(format!("authored def lookup: {e}")))?;
        Ok(lpc_wire::snapshot_slot_shape(shape, data, self.slot_shapes))
    }

    fn read_authored_def_product_by_accessor(
        &self,
        node: NodeId,
        accessor: &SlotAccessor,
    ) -> Result<SlotData, SessionResolveError> {
        let def = self.loaded_node_def(node)?;
        let data = accessor
            .access(def, self.slot_shapes)
            .map_err(|e| SessionResolveError::other(format!("authored def accessor: {e}")))?;
        let (_, shape) = lookup_slot_data_and_shape(def, self.slot_shapes, accessor.path())
            .map_err(|e| SessionResolveError::other(format!("authored def accessor shape: {e}")))?;
        Ok(lpc_wire::snapshot_slot_shape(shape, data, self.slot_shapes))
    }

    fn read_shader_consumed_slot_merge_policy(
        &self,
        node: NodeId,
        slot: &SlotPath,
    ) -> Result<Option<SlotMerge>, SessionResolveError> {
        Ok(self
            .shader_consumed_slot_def(node, slot)?
            .map(|slot| match slot.kind.value() {
                lpc_model::ShaderSlotKind::Map => SlotMerge::ByKey,
                // A timebase or palette uniform's binding, when it has one,
                // names a single config channel — never an aggregate.
                lpc_model::ShaderSlotKind::Value
                | lpc_model::ShaderSlotKind::Phasor
                | lpc_model::ShaderSlotKind::Seconds
                | lpc_model::ShaderSlotKind::Palette => SlotMerge::Latest,
            }))
    }

    /// The authored default an *unbound* shader uniform runs on, as slot
    /// data — `None` when `slot` does not name a shader consumed slot.
    ///
    /// Shader uniforms are not fields of `ShaderDef`/`ComputeShaderDef`;
    /// they live in the `consumed` map, keyed by uniform name. So a
    /// `read_authored_def_product` lookup of the *name* has nothing to hit
    /// and every unbound uniform came back `UnresolvedConsumedSlot` — a
    /// node behaving exactly as authored reported a permanent `Warn`,
    /// indistinguishable from a genuinely broken binding
    /// (`docs/defects/2026-08-04-unbound-shader-uniform-warns.md`).
    ///
    /// The data produced is what [`materialize_shader_input`] already
    /// builds for absent data, so the *value* an unbound uniform runs on is
    /// unchanged: a value slot's authored `default` (0.0 when unauthored),
    /// and an empty map for a map slot — which fills every element with the
    /// mapping's sentinel key.
    ///
    /// Only the plain [`QueryKey::ConsumedSlot`] path needs this. An
    /// accessor is compiled against the def's own shape, so a uniform name
    /// can never become one in the first place.
    ///
    /// [`materialize_shader_input`]: crate::nodes::shader::shader_input_materialize::materialize_shader_input
    fn read_shader_consumed_slot_default(
        &self,
        node: NodeId,
        slot: &SlotPath,
    ) -> Result<Option<SlotData>, SessionResolveError> {
        Ok(self
            .shader_consumed_slot_def(node, slot)?
            .map(|slot| match slot.kind.value() {
                // Timebase kinds are scalar f32 on the wire; their real
                // value comes from the scope's time product at uniform
                // fill, so the unbound default is only ever a placeholder.
                lpc_model::ShaderSlotKind::Value
                | lpc_model::ShaderSlotKind::Phasor
                | lpc_model::ShaderSlotKind::Seconds => {
                    SlotData::Value(WithRevision::new(self.frame_revision, slot.default_value()))
                }
                // A palette's authored default is its whole `GradientConfig`
                // — the same value shape a `bus:palette` channel carries, so
                // the bake path reads one thing whether or not anything
                // drives it (`docs/design/color.md` §5).
                lpc_model::ShaderSlotKind::Palette => SlotData::Value(WithRevision::new(
                    self.frame_revision,
                    lpc_model::ToLpValue::to_lp_value(&slot.gradient_config()),
                )),
                lpc_model::ShaderSlotKind::Map => {
                    SlotData::Map(lpc_model::SlotMapDyn::with_revision(
                        self.frame_revision,
                        lp_collection::VecMap::new(),
                    ))
                }
            }))
    }

    /// The authored `consumed[<name>]` def `slot` names, when `node` is a
    /// shader or compute shader and `slot` is a bare uniform name.
    fn shader_consumed_slot_def(
        &self,
        node: NodeId,
        slot: &SlotPath,
    ) -> Result<Option<&lpc_model::ShaderSlotDef>, SessionResolveError> {
        let Some(SlotPathSegment::Field(name)) = slot.segments().first() else {
            return Ok(None);
        };
        if slot.segments().len() != 1 {
            return Ok(None);
        }
        let def = self.loaded_node_def(node)?;
        Ok(match def {
            NodeDef::Shader(config) => config.consumed_slots.entries.get(name.as_str()),
            NodeDef::ComputeShader(config) => config.consumed_slots.entries.get(name.as_str()),
            _ => None,
        })
    }

    fn read_authored_def_slot_semantics(
        &self,
        node: NodeId,
        slot: &SlotPath,
    ) -> Result<SlotSemantics, SessionResolveError> {
        let def = self.loaded_node_def(node)?;
        let shape = self.slot_shapes.get_shape(def.shape_id()).ok_or_else(|| {
            SessionResolveError::other(format!("missing node def shape {}", def.shape_id()))
        })?;
        slot_path_semantics(shape, self.slot_shapes, slot)
    }

    fn loaded_node_def(&self, node: NodeId) -> Result<&NodeDef, SessionResolveError> {
        let entry = self
            .tree
            .get(node)
            .ok_or_else(|| SessionResolveError::other(format!("unknown node {node:?}")))?;
        let location = entry.def_location.as_ref().ok_or_else(|| {
            SessionResolveError::other(format!("node {node:?} has no project definition location"))
        })?;
        loaded_registry_def(self.registry, location)
    }

    /// Answer the product-space query (plan D17) by routing it to the
    /// producing node, exactly like a render call.
    ///
    /// The node is taken out of the tree for the duration for the same
    /// reason the render paths take it: a forwarding producer (playlist,
    /// module) answers by asking the engine about *its* upstream product,
    /// which re-enters this host.
    fn visual_node_space(
        &mut self,
        product: VisualProduct,
    ) -> Result<ProductSpaceInfo, SessionResolveError> {
        let node_id = product.node();
        let revision = self.frame_revision;
        let mut node_runtime = {
            let entry = self.tree.get_mut(node_id).ok_or_else(|| {
                SessionResolveError::other(format!("visual space: unknown node {node_id:?}"))
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
                other => {
                    let already_executing = matches!(&other, NodeEntryState::Executing { .. })
                        .then(|| match &other {
                            NodeEntryState::Executing { call } => call.call.label(),
                            _ => unreachable!("checked by the matches! above"),
                        });
                    entry.state = WithRevision::new(old_changed_at, other);
                    return Err(SessionResolveError::other(match already_executing {
                        Some(label) => format!(
                            "node {node_id:?} is already executing {label}; re-entry through EngineSession is unsupported"
                        ),
                        None => format!("visual space: node {node_id:?} not alive"),
                    }));
                }
            }
        };

        let recovery_name = recovery_frame_name(&self.tree, node_id);
        let result = {
            match node_runtime.render_node() {
                Some(render_node) => {
                    let mut ctx = RenderContext::with_services(
                        node_id,
                        revision,
                        self.graphics.clone(),
                        self.time_provider.clone(),
                        self.frame_time_seconds,
                        self,
                    );
                    catch_node_panic_framed(
                        lp_recovery::FrameKind::NodeRender,
                        &recovery_name,
                        || render_node.visual_space(product, &mut ctx),
                    )
                }
                // Not a render node at all: nothing to project, and the
                // caller's real error comes from the render call itself.
                None => Ok(ProductSpaceInfo::two_d()),
            }
        };

        let entry = self.tree.get_mut(node_id).ok_or_else(|| {
            SessionResolveError::other(format!("visual space: unknown node {node_id:?}"))
        })?;
        entry.set_state(NodeEntryState::Alive(node_runtime), revision);
        result.map_err(|e| SessionResolveError::other(format!("visual space: {e}")))
    }

    fn render_node_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, SessionResolveError> {
        let node_id = product.node();
        let revision = self.frame_revision;
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

        let recovery_name = recovery_frame_name(&self.tree, node_id);
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
            catch_node_panic_framed(lp_recovery::FrameKind::NodeRender, &recovery_name, || {
                render_node.render_texture(product, request, &mut ctx)
            })
        };

        let entry = self.tree.get_mut(node_id).ok_or_else(|| {
            SessionResolveError::other(format!("render: unknown node {node_id:?}"))
        })?;
        let runtime_status = runtime_status_or_ok(&*node_runtime);
        entry.set_state(NodeEntryState::Alive(node_runtime), revision);

        match result {
            Ok(product) => {
                set_entry_status_if_changed(entry, runtime_status, revision);
                Ok(product)
            }
            Err(e) => {
                let message = e.to_string();
                set_entry_status_if_changed(
                    entry,
                    NodeRuntimeStatus::Error(message.clone()),
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
        target: &mut TextureHandle,
    ) -> Result<(), SessionResolveError> {
        let node_id = product.node();
        let revision = self.frame_revision;
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

        let recovery_name = recovery_frame_name(&self.tree, node_id);
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
            catch_node_panic_framed(lp_recovery::FrameKind::NodeRender, &recovery_name, || {
                render_node.render_texture_into(product, request, target, &mut ctx)
            })
        };

        let entry = self.tree.get_mut(node_id).ok_or_else(|| {
            SessionResolveError::other(format!("render: unknown node {node_id:?}"))
        })?;
        let runtime_status = runtime_status_or_ok(&*node_runtime);
        entry.set_state(NodeEntryState::Alive(node_runtime), revision);

        match result {
            Ok(()) => {
                set_entry_status_if_changed(entry, runtime_status, revision);
                Ok(())
            }
            Err(e) => {
                let message = e.to_string();
                set_entry_status_if_changed(
                    entry,
                    NodeRuntimeStatus::Error(message.clone()),
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
        let revision = self.frame_revision;
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

        let recovery_name = recovery_frame_name(&self.tree, node_id);
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
            catch_node_panic_framed(lp_recovery::FrameKind::NodeRender, &recovery_name, || {
                render_node.sample_visual_into(product, request, target, &mut ctx)
            })
        };

        let entry = self.tree.get_mut(node_id).ok_or_else(|| {
            SessionResolveError::other(format!("sample visual: unknown node {node_id:?}"))
        })?;
        let runtime_status = runtime_status_or_ok(&*node_runtime);
        entry.set_state(NodeEntryState::Alive(node_runtime), revision);

        match result {
            Ok(()) => {
                set_entry_status_if_changed(entry, runtime_status, revision);
                Ok(())
            }
            Err(e) => {
                let message = e.to_string();
                set_entry_status_if_changed(
                    entry,
                    NodeRuntimeStatus::Error(message.clone()),
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
        let revision = self.frame_revision;
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

        let recovery_name = recovery_frame_name(&self.tree, node_id);
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
                self.safe_output_clamp_q16,
                self,
            );
            catch_node_panic_framed(lp_recovery::FrameKind::NodeRender, &recovery_name, || {
                control_node.render_control(product, request, target, &mut ctx)
            })
        };

        let entry = self.tree.get_mut(node_id).ok_or_else(|| {
            SessionResolveError::other(format!("control render: unknown node {node_id:?}"))
        })?;
        let runtime_status = runtime_status_or_ok(&*node_runtime);
        entry.set_state(NodeEntryState::Alive(node_runtime), revision);

        match result {
            Ok(layout) => {
                set_entry_status_if_changed(entry, runtime_status, revision);
                Ok(layout)
            }
            Err(e) => {
                let message = e.to_string();
                set_entry_status_if_changed(
                    entry,
                    NodeRuntimeStatus::Error(message.clone()),
                    revision,
                );
                Err(SessionResolveError::other(format!(
                    "control render: {message}"
                )))
            }
        }
    }

    fn render_node_control_probe(
        &mut self,
        product: ControlProduct,
        request: &ControlRenderRequest,
        target: ControlRenderTarget<'_>,
        display_layout: ControlDisplayLayoutRead,
    ) -> Result<(ControlLayout, ControlDisplayLayoutProbeResult), SessionResolveError> {
        let node_id = product.node();
        let revision = self.frame_revision;
        let mut node_runtime = {
            let entry = self.tree.get_mut(node_id).ok_or_else(|| {
                SessionResolveError::other(format!(
                    "control product probe: unknown node {node_id:?}"
                ))
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
                        "control product probe: node {node_id:?} not alive"
                    )));
                }
            }
        };

        let recovery_name = recovery_frame_name(&self.tree, node_id);
        let result = {
            let Some(control_node) = node_runtime.control_node() else {
                return restore_node_after_failed_control_probe(
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
                self.safe_output_clamp_q16,
                self,
            );
            catch_node_panic_framed(lp_recovery::FrameKind::NodeRender, &recovery_name, || {
                let sample_layout =
                    control_node.render_control(product, request, target, &mut ctx)?;
                let display_layout =
                    control_display_layout_result(control_node, product, display_layout, &mut ctx)?;
                Ok((sample_layout, display_layout))
            })
        };

        let entry = self.tree.get_mut(node_id).ok_or_else(|| {
            SessionResolveError::other(format!("control product probe: unknown node {node_id:?}"))
        })?;
        let runtime_status = runtime_status_or_ok(&*node_runtime);
        entry.set_state(NodeEntryState::Alive(node_runtime), revision);

        match result {
            Ok(probe) => {
                set_entry_status_if_changed(entry, runtime_status, revision);
                Ok(probe)
            }
            Err(e) => {
                let message = e.to_string();
                set_entry_status_if_changed(
                    entry,
                    NodeRuntimeStatus::Error(message.clone()),
                    revision,
                );
                Err(SessionResolveError::other(format!(
                    "control product probe: {message}"
                )))
            }
        }
    }

    /// The geometry-only half of [`Self::render_node_control_probe`].
    ///
    /// Steals the producer exactly like the render probe does — the same
    /// re-entrancy guard applies, since `control_display_layout` takes
    /// `&mut self` on the node — but calls ONLY
    /// [`crate::node::ControlNode::control_display_layout`]. No render, no
    /// target, no samples touched.
    fn node_control_display_layout(
        &mut self,
        product: ControlProduct,
        request: ControlDisplayLayoutRead,
    ) -> Result<ControlDisplayLayoutProbeResult, SessionResolveError> {
        let node_id = product.node();
        let revision = self.frame_revision;
        let mut node_runtime = {
            let entry = self.tree.get_mut(node_id).ok_or_else(|| {
                SessionResolveError::other(format!(
                    "display layout probe: unknown node {node_id:?}"
                ))
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
                other => {
                    let executing = other.is_executing();
                    entry.state = WithRevision::new(old_changed_at, other);
                    return Err(SessionResolveError::other(if executing {
                        format!(
                            "display layout probe: node {node_id:?} is already executing; \
                             re-entry through EngineSession is unsupported"
                        )
                    } else {
                        format!("display layout probe: node {node_id:?} not alive")
                    }));
                }
            }
        };

        let recovery_name = recovery_frame_name(&self.tree, node_id);
        let result = {
            let Some(control_node) = node_runtime.control_node() else {
                if let Some(entry) = self.tree.get_mut(node_id) {
                    entry.set_state(NodeEntryState::Alive(node_runtime), revision);
                }
                return Ok(ControlDisplayLayoutProbeResult::Unsupported {
                    reason: format!(
                        "node {node_id:?} renders no control product output {}",
                        product.output()
                    ),
                });
            };
            let mut ctx = ControlRenderContext::new(
                node_id,
                revision,
                self.graphics.clone(),
                self.frame_time_seconds,
                self.safe_output_clamp_q16,
                self,
            );
            catch_node_panic_framed(lp_recovery::FrameKind::NodeRender, &recovery_name, || {
                control_display_layout_result(control_node, product, request, &mut ctx)
            })
        };

        let entry = self.tree.get_mut(node_id).ok_or_else(|| {
            SessionResolveError::other(format!("display layout probe: unknown node {node_id:?}"))
        })?;
        entry.set_state(NodeEntryState::Alive(node_runtime), revision);

        // A layout failure is NOT a node health event: the read is a passive
        // observer of a node the tick is driving fine, so it reports the
        // refusal in-band and leaves the node's status alone.
        result.map_err(|e| SessionResolveError::other(format!("display layout probe: {e}")))
    }
}

/// The most bytes a serialized display layout may occupy and still ride one
/// project-read frame alongside its probe header and frame envelope.
///
/// The transport rejects any single event larger than
/// [`lpc_wire::PROJECT_READ_FRAME_MAX_BYTES`], and that rejection is terminal
/// for the whole read stream — an over-budget layout wedges the entire
/// project view, not just one probe. Until layouts stream in bounded chunks
/// (the "semantic layout split" escalation noted in
/// `lpc-wire`'s probe tests), an oversized layout is refused here as
/// `Unsupported`, which clients already render as a graceful fallback. The
/// 2 KiB margin covers the probe header (extent, sample layout, product) and
/// the frame envelope around the event.
const DISPLAY_LAYOUT_WIRE_BUDGET: usize = lpc_wire::PROJECT_READ_FRAME_MAX_BYTES - 2048;

fn control_display_layout_result(
    control_node: &mut dyn crate::node::ControlNode,
    product: ControlProduct,
    request: ControlDisplayLayoutRead,
    ctx: &mut ControlRenderContext<'_>,
) -> Result<ControlDisplayLayoutProbeResult, NodeError> {
    match request {
        ControlDisplayLayoutRead::None => Ok(ControlDisplayLayoutProbeResult::Omitted),
        ControlDisplayLayoutRead::Always | ControlDisplayLayoutRead::IfChanged { .. } => {
            let Some(layout) = control_node.control_display_layout(product, ctx)? else {
                return Ok(ControlDisplayLayoutProbeResult::Unsupported {
                    reason: alloc::string::String::from(
                        "control product does not expose display layout",
                    ),
                });
            };
            let revision = layout.revision();
            match request {
                ControlDisplayLayoutRead::IfChanged {
                    known_revision: Some(known),
                } if known == revision => {
                    Ok(ControlDisplayLayoutProbeResult::Unchanged { revision })
                }
                _ => {
                    let layout_len = lpc_wire::ser_write_json_len(&layout);
                    if layout_len > DISPLAY_LAYOUT_WIRE_BUDGET {
                        return Ok(ControlDisplayLayoutProbeResult::Unsupported {
                            reason: alloc::format!(
                                "display layout is {layout_len} bytes serialized, over the \
                                 {DISPLAY_LAYOUT_WIRE_BUDGET}-byte wire budget; layouts this \
                                 large need chunked streaming (not yet implemented)"
                            ),
                        });
                    }
                    Ok(ControlDisplayLayoutProbeResult::Layout(layout))
                }
            }
        }
    }
}

fn slot_path_semantics(
    shape: SlotShapeView<'_>,
    registry: &(impl SlotShapeLookup + ?Sized),
    slot: &SlotPath,
) -> Result<SlotSemantics, SessionResolveError> {
    slot_path_semantics_segments(shape, registry, slot, slot.segments())
}

fn slot_path_semantics_segments(
    shape: SlotShapeView<'_>,
    registry: &(impl SlotShapeLookup + ?Sized),
    slot: &SlotPath,
    segments: &[SlotPathSegment],
) -> Result<SlotSemantics, SessionResolveError> {
    let shape = resolve_shape_projection(shape, registry)?;
    let Some((head, tail)) = segments.split_first() else {
        return Err(SessionResolveError::other(format!(
            "slot path {slot} does not identify a record field"
        )));
    };

    match head {
        SlotPathSegment::Field(name) if shape.record_field_by_name(name).is_some() => {
            let (_, field) = shape
                .record_field_by_name(name)
                .expect("field checked above");
            if tail.is_empty() {
                Ok(field.semantics())
            } else {
                slot_path_semantics_segments(field.shape(), registry, slot, tail)
            }
        }
        SlotPathSegment::Key(_) if shape.map_value().is_some() => {
            let value = shape.map_value().expect("map value checked above");
            slot_path_semantics_segments(value, registry, slot, tail)
        }
        SlotPathSegment::Field(name)
            if name.as_str() == "some" && shape.option_some().is_some() =>
        {
            let some = shape.option_some().expect("option some checked above");
            slot_path_semantics_segments(some, registry, slot, tail)
        }
        SlotPathSegment::Field(name) if shape.enum_variant_by_name(name).is_some() => {
            let variant = shape.enum_variant_by_name(name).ok_or_else(|| {
                SessionResolveError::other(format!("node def enum has no variant {name}"))
            })?;
            slot_path_semantics_segments(variant.shape(), registry, slot, tail)
        }
        SlotPathSegment::Field(name) => Err(SessionResolveError::other(format!(
            "slot path field {name} cannot descend through node def shape for {slot}"
        ))),
        SlotPathSegment::Key(key) => Err(SessionResolveError::other(format!(
            "slot path key {key:?} cannot descend through node def shape for {slot}"
        ))),
    }
}

/// The declared panel hint of the slot a binding consumes: the hint sits on
/// the TOP-LEVEL declared field (`brightness.some` reads field
/// `brightness`) in the node's def shape — or, for a shader's dynamic
/// slots, as the `panel` field of the slot's authored def (a shader slot is
/// authored data, so it spells the hint as a value where a native def
/// spells it as `#[slot(panel = "show")]` shape metadata). `None` for
/// undeclared slots and for defs that are not loaded.
pub(crate) fn authored_def_slot_panel_hint(
    registry: &ProjectRegistry,
    slot_shapes: &SlotShapeRegistry,
    location: &NodeDefLocation,
    slot: &SlotPath,
) -> Option<lpc_model::PanelHint> {
    let def = loaded_registry_def(registry, location).ok()?;
    let SlotPathSegment::Field(name) = slot.segments().first()? else {
        return None;
    };
    let dynamic_slots = match def {
        NodeDef::Shader(shader) => Some(&shader.consumed_slots),
        NodeDef::ComputeShader(compute) => Some(&compute.consumed_slots),
        _ => None,
    };
    if let Some(slots) = dynamic_slots {
        return slots.entries.get(name.as_str())?.panel_hint();
    }
    let shape = slot_shapes.get_shape(def.shape_id())?;
    let shape = resolve_shape_projection(shape, slot_shapes).ok()?;
    let (_, field) = shape.record_field_by_name(name)?;
    field.panel()
}

fn resolve_shape_projection<'a>(
    shape: SlotShapeView<'a>,
    registry: &'a (impl SlotShapeLookup + ?Sized),
) -> Result<SlotShapeView<'a>, SessionResolveError> {
    let mut shape = shape;
    loop {
        if let Some(id) = shape.ref_id() {
            shape = registry.get_shape(id).ok_or_else(|| {
                SessionResolveError::other(format!("missing referenced node def shape {id}"))
            })?;
        } else if let Some(projected) = shape.custom_shape() {
            shape = projected;
        } else {
            return Ok(shape);
        }
    }
}

impl ControlRenderServices for EngineResolveHost<'_> {
    fn visual_product_space(
        &mut self,
        product: VisualProduct,
    ) -> Result<ProductSpaceInfo, NodeError> {
        self.visual_node_space(product)
            .map_err(|e| NodeError::msg(format!("visual space: {e}")))
    }

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
        target: &mut TextureHandle,
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
    fn visual_product_space(
        &mut self,
        product: VisualProduct,
    ) -> Result<ProductSpaceInfo, NodeError> {
        self.visual_node_space(product)
            .map_err(|e| NodeError::msg(format!("visual space: {e}")))
    }

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
        target: &mut TextureHandle,
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
    tree: &mut RuntimeNodeTree<Box<dyn NodeRuntime>>,
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

/// Display/identity name for a node's recovery frame: its stable tree path.
/// The path (not the numeric id) keys crash blame, so it must survive
/// project reloads and reboots.
fn recovery_frame_name<N>(tree: &RuntimeNodeTree<N>, node_id: NodeId) -> alloc::string::String {
    tree.get(node_id)
        .map(|entry| entry.path.to_string())
        .unwrap_or_default()
}

fn set_entry_status_if_changed<N>(
    entry: &mut RuntimeNodeEntry<N>,
    status: NodeRuntimeStatus,
    revision: Revision,
) {
    if entry.status.value() != &status {
        entry.set_status(status, revision);
    }
}

fn runtime_status_or_ok(node: &dyn NodeRuntime) -> NodeRuntimeStatus {
    node.runtime_status().unwrap_or(NodeRuntimeStatus::Ok)
}

fn restore_node_after_failed_render_unit(
    tree: &mut RuntimeNodeTree<Box<dyn NodeRuntime>>,
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
    tree: &mut RuntimeNodeTree<Box<dyn NodeRuntime>>,
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

fn restore_node_after_failed_control_probe(
    tree: &mut RuntimeNodeTree<Box<dyn NodeRuntime>>,
    node_id: NodeId,
    node_runtime: Box<dyn NodeRuntime>,
    revision: Revision,
    err: SessionResolveError,
) -> Result<(ControlLayout, ControlDisplayLayoutProbeResult), SessionResolveError> {
    if let Some(entry) = tree.get_mut(node_id) {
        entry.set_state(NodeEntryState::Alive(node_runtime), revision);
    }
    Err(err)
}

fn consume_tree_node(
    session: &mut EngineSession<'_>,
    host: &mut EngineResolveHost<'_>,
    node_id: NodeId,
) -> Result<(), EngineError> {
    let revision = session.revision();
    let restore_frame = session.revision();
    let mut node_runtime = {
        let entry = host
            .tree
            .get_mut(node_id)
            .ok_or(EngineError::UnknownNode(node_id))?;

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
        node_runtime
    };

    let gfx = host.graphics.clone();
    let time_provider = host.time_provider.clone();
    let button_service = host.button_service.clone();
    let radio_service = host.radio_service.clone();
    let time_s = host.frame_time_seconds;
    let slot_shapes = host.slot_shapes;
    let recovery_name = recovery_frame_name(&host.tree, node_id);
    let consume_result = {
        let mut bridge = SessionHostResolver {
            session,
            host: host as &mut dyn ResolveHost,
        };
        let resolver_dyn: &mut dyn TickResolver = &mut bridge;
        let mut tick_ctx = TickContext::with_engine_services(
            node_id,
            revision,
            resolver_dyn,
            slot_shapes,
            gfx,
            time_provider,
            button_service,
            radio_service,
            time_s,
        );
        catch_node_panic_framed(lp_recovery::FrameKind::NodeRender, &recovery_name, || {
            node_runtime.consume(&mut tick_ctx)
        })
    };

    let entry = host
        .tree
        .get_mut(node_id)
        .ok_or(EngineError::UnknownNode(node_id))?;
    let runtime_status = runtime_status_or_ok(&*node_runtime);
    entry.set_state(NodeEntryState::Alive(node_runtime), restore_frame);

    match consume_result {
        Ok(()) => {
            set_entry_status_if_changed(entry, runtime_status, revision);
            host.producers_ticked.insert(node_id);
            Ok(())
        }
        Err(e) => {
            let message = e.to_string();
            set_entry_status_if_changed(entry, NodeRuntimeStatus::Error(message.clone()), revision);
            Err(EngineError::Node {
                node: node_id,
                message,
            })
        }
    }
}

fn loaded_registry_def<'a>(
    registry: &'a ProjectRegistry,
    location: &NodeDefLocation,
) -> Result<&'a NodeDef, SessionResolveError> {
    let entry = registry.def(location).ok_or_else(|| {
        SessionResolveError::other(format!("node definition {location:?} is not in inventory"))
    })?;
    match &entry.state {
        NodeDefState::Loaded(def) => Ok(def),
        other => Err(SessionResolveError::other(format!(
            "node definition {location:?} has no loaded payload: {other:?}"
        ))),
    }
}

#[cfg(test)]
pub(crate) fn resolve_with_engine_host(
    eng: &mut Engine,
    registry: &ProjectRegistry,
    key: QueryKey,
    log_level: ResolveLogLevel,
) -> Result<(Production, ResolveTrace), SessionResolveError> {
    let fid = eng.revision;
    let mut resolver_tmp = core::mem::replace(&mut eng.resolver, Resolver::new());
    resolver_tmp.begin_frame();
    let mut session = EngineSession::new(fid, &mut resolver_tmp, ResolveTrace::new(log_level));
    let mut producers_ticked = VecSet::new();
    let time_s = eng.frame_time.total_ms as f32 / 1000.0;
    let time_provider = eng.services.time_provider();
    let button_service = eng.services.button_service();
    let radio_service = eng.services.radio_service();
    let mut host = EngineResolveHost {
        tree: &mut eng.tree,
        registry,
        panel_writers: &eng.panel_writers,
        timebases: &mut eng.timebases,
        producers_ticked: &mut producers_ticked,
        runtime_buffers: &mut eng.runtime_buffers,
        slot_shapes: &eng.slot_shapes,
        graphics: eng.graphics.clone(),
        time_provider,
        button_service,
        radio_service,
        frame_time_seconds: time_s,
        safe_output_clamp_q16: eng.safe_output_clamp_q16,
        frame_revision: eng.revision,
    };
    let result = session
        .resolve(&mut host, &key)
        .map(|pv| (pv, session.trace().clone()));
    eng.resolver = resolver_tmp;
    result
}

#[cfg(test)]
pub(super) fn resolve_twice_same_frame_with_engine_host(
    eng: &mut Engine,
    registry: &ProjectRegistry,
    key: QueryKey,
) -> Result<(Production, Production), SessionResolveError> {
    let fid = eng.revision;
    let mut resolver_tmp = core::mem::replace(&mut eng.resolver, Resolver::new());
    resolver_tmp.begin_frame();
    let mut session = EngineSession::new(
        fid,
        &mut resolver_tmp,
        ResolveTrace::new(ResolveLogLevel::Off),
    );
    let mut producers_ticked = VecSet::new();
    let time_s = eng.frame_time.total_ms as f32 / 1000.0;
    let time_provider = eng.services.time_provider();
    let button_service = eng.services.button_service();
    let radio_service = eng.services.radio_service();
    let mut host = EngineResolveHost {
        tree: &mut eng.tree,
        registry,
        panel_writers: &eng.panel_writers,
        timebases: &mut eng.timebases,
        producers_ticked: &mut producers_ticked,
        runtime_buffers: &mut eng.runtime_buffers,
        slot_shapes: &eng.slot_shapes,
        graphics: eng.graphics.clone(),
        time_provider,
        button_service,
        radio_service,
        frame_time_seconds: time_s,
        safe_output_clamp_q16: eng.safe_output_clamp_q16,
        frame_revision: eng.revision,
    };
    let result = session.resolve(&mut host, &key).and_then(|first| {
        session
            .resolve(&mut host, &key)
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
        let registry = ProjectRegistry::new();
        let initial_revision = eng.revision();
        eng.tick(&registry, 10).expect("tick");
        assert_eq!(eng.frame_num(), FrameNum::new(1));
        assert!(eng.revision() > initial_revision);
        assert_eq!(eng.frame_time().delta_ms, 10);
        assert_eq!(eng.frame_time().total_ms, 10);
        let first_tick_revision = eng.revision();
        eng.tick(&registry, 5).expect("tick");
        assert_eq!(eng.frame_num(), FrameNum::new(2));
        assert!(eng.revision() > first_tick_revision);
        assert_eq!(eng.frame_time().total_ms, 15);
    }

    #[test]
    fn tick_error_sets_node_status_and_restores_runtime() {
        let mut eng = Engine::new(TreePath::parse("/show.t").expect("path"));
        let registry = ProjectRegistry::new();
        let root = eng.tree().root();
        let cfg = test_placeholder_spine();
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

        let err = eng.tick(&registry, 10).expect_err("tick should fail");
        assert!(err.to_string().contains("intentional tick failure"));

        let entry = eng.tree().get(node).expect("entry");
        assert!(matches!(entry.state.value(), NodeEntryState::Alive(_)));
        assert!(matches!(
            entry.status.value(),
            NodeRuntimeStatus::Error(message) if message == "intentional tick failure"
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

        h.tick(1).expect("tick");

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
        h.tick(1).expect("tick");
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

        let (first, second) =
            super::resolve_twice_same_frame_with_engine_host(&mut h.engine, &h.registry, key)
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
    fn playlist_root_trigger_semantics_are_consumed_by_key() {
        let registry = SlotShapeRegistry::default();
        let shape = <lpc_model::PlaylistDef as lpc_model::StaticSlotShape>::slot_shape();
        let semantics = slot_path_semantics(
            SlotShapeView::Dynamic(&shape),
            &registry,
            &SlotPath::parse("trigger").expect("trigger path"),
        )
        .expect("trigger semantics");

        assert_eq!(semantics.direction, SlotDirection::Consumed);
        assert_eq!(semantics.merge, SlotMerge::ByKey);
    }

    #[test]
    fn nested_slot_semantics_walk_map_entries() {
        let registry = SlotShapeRegistry::default();
        let shape = <lpc_model::PlaylistDef as lpc_model::StaticSlotShape>::slot_shape();
        let semantics = slot_path_semantics(
            SlotShapeView::Dynamic(&shape),
            &registry,
            &SlotPath::parse("entries[2].trigger_ids").expect("trigger_ids path"),
        )
        .expect("trigger_ids semantics");

        assert_eq!(semantics.direction, SlotDirection::Local);
        assert_eq!(semantics.merge, SlotMerge::Latest);
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
            .resolve_with_trace(QueryKey::Bus {
                scope: None,
                channel: lpc_model::ChannelName(String::from("video")),
            })
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
        fn consume(&mut self, _ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
            Err(NodeError::msg("intentional tick failure"))
        }

        fn destroy(&mut self, _ctx: &mut crate::node::DestroyCtx) -> Result<(), NodeError> {
            Ok(())
        }

        fn handle_memory_pressure(
            &mut self,
            _level: crate::node::PressureLevel,
            _ctx: &mut crate::node::MemPressureCtx,
        ) -> Result<(), NodeError> {
            Ok(())
        }
    }

    #[test]
    #[cfg(feature = "node-playlist")]
    fn node_command_dispatches_to_the_addressed_runtime() {
        use crate::nodes::{PlaylistNode, PlaylistRuntimeEntry};
        use lpc_wire::WireNodeCommand;

        let mut eng = Engine::new(TreePath::parse("/show.t").expect("path"));
        let root = eng.tree().root();
        let node = eng
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("playlist").expect("name"),
                lpc_model::NodeName::parse("playlist").expect("kind"),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                test_placeholder_spine(),
                Revision::new(1),
            )
            .expect("add node");
        let entries = alloc::vec![PlaylistRuntimeEntry {
            index: 1,
            child: NodeId::new(99),
            output_slot: SlotPath::parse("output").expect("path"),
            duration: None,
            fade_after: None,
            trigger_ids: None,
        }];
        eng.attach_runtime_node(
            node,
            Box::new(PlaylistNode::new(node, 1, 0.0, entries)),
            Revision::new(1),
        )
        .expect("attach node");

        eng.handle_node_command(node, &WireNodeCommand::PlaylistActivateEntry { entry: 1 })
            .expect("known entry accepted");

        let err = eng
            .handle_node_command(node, &WireNodeCommand::PlaylistActivateEntry { entry: 9 })
            .expect_err("unknown entry rejected");
        assert!(err.to_string().contains("no loaded entry 9"), "{err}");

        let err = eng
            .handle_node_command(
                NodeId::new(4242),
                &WireNodeCommand::PlaylistActivateEntry { entry: 1 },
            )
            .expect_err("unknown node rejected");
        assert!(matches!(err, EngineError::UnknownNode(_)));
    }

    /// Nodes without a `handle_command` override reject every command —
    /// the channel is opt-in per runtime.
    #[test]
    fn node_command_default_is_rejected() {
        use lpc_wire::WireNodeCommand;

        let mut node = FailingNode;
        let err = NodeRuntime::handle_command(
            &mut node,
            &WireNodeCommand::PlaylistActivateEntry { entry: 0 },
            0.0,
        )
        .expect_err("default rejects");
        assert!(err.to_string().contains("accepts no runtime commands"));
    }
}
