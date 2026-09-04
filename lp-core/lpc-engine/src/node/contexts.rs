//! Narrow contexts passed into [`super::NodeRuntime`] hooks.
//!
//! [`TickContext`] resolves through the active [`ResolveSession`] and [`ResolveHost`] using
//! [`QueryKey`] (not the legacy slot resolver cache).

use alloc::rc::Rc;
use alloc::sync::Arc;

use crate::dataflow::resolver::{
    Production, ProductionSource, QueryKey, ResolveError, TickResolver,
};
use crate::dataflow::timebase::PhasorKey;
use crate::engine::{ButtonService, FaultPresentation, RadioService};
use crate::products::control::{
    ControlLayout, ControlProduct, ControlRenderRequest, ControlRenderTarget,
};
use crate::products::visual::{
    ProductSpaceInfo, RenderTextureRequest, TextureRenderProduct, VisualProduct,
    VisualSampleBufferRequest, VisualSampleTarget,
};
use crate::resource::{RuntimeBuffer, RuntimeBufferId, RuntimeBufferStore};
use lp_gfx::{LpGraphics, TextureHandle};
use lpc_model::{
    AssetLocation, FromLpValue, NodeId, PhasorConfig, Revision, SlotAccess, SlotAccessor, SlotPath,
    SlotShapeRegistry, TimeProduct, WithRevision, lookup_slot_data_and_shape,
};
use lpc_registry::{AssetBytes, AssetReadError, AssetText, ProjectRegistry};
use lpc_shared::time::TimeProvider;
use lpfs::LpFs;

use super::ScopeRef;
use super::node_error::NodeError;
use super::node_runtime::PatchedRun;

/// Narrow store access for allocating node-owned visual products and runtime buffers at attach time.
///
/// Passed to [`super::super::NodeRuntime::init_resources`] before the node payload is [`crate::node::NodeEntryState::Alive`].
pub struct NodeResourceInitContext<'a> {
    node_id: NodeId,
    runtime_buffers: &'a mut RuntimeBufferStore,
}

impl<'a> NodeResourceInitContext<'a> {
    pub fn new(node_id: NodeId, runtime_buffers: &'a mut RuntimeBufferStore) -> Self {
        Self {
            node_id,
            runtime_buffers,
        }
    }

    pub fn insert_runtime_buffer(
        &mut self,
        buffer: WithRevision<RuntimeBuffer>,
    ) -> RuntimeBufferId {
        self.runtime_buffers.insert_owned(self.node_id, buffer)
    }
}

/// Context for [`super::NodeRuntime::refresh_asset`].
pub struct AssetRefreshContext<'a> {
    fs: &'a dyn LpFs,
    registry: &'a mut ProjectRegistry,
    slot_shapes: &'a SlotShapeRegistry,
    revision: Revision,
}

impl<'a> AssetRefreshContext<'a> {
    pub fn new(
        fs: &'a dyn LpFs,
        registry: &'a mut ProjectRegistry,
        slot_shapes: &'a SlotShapeRegistry,
        revision: Revision,
    ) -> Self {
        Self {
            fs,
            registry,
            slot_shapes,
            revision,
        }
    }

    pub fn fs(&self) -> &dyn LpFs {
        self.fs
    }

    pub fn registry(&mut self) -> &mut ProjectRegistry {
        self.registry
    }

    pub fn slot_shapes(&self) -> &SlotShapeRegistry {
        self.slot_shapes
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn read_asset_bytes_if_changed(
        &mut self,
        location: &AssetLocation,
        since: Revision,
    ) -> Result<Option<AssetBytes>, AssetReadError> {
        self.registry
            .read_asset_bytes_if_changed(self.fs, location, since)
    }

    pub fn read_asset_text_if_changed(
        &mut self,
        location: &AssetLocation,
        since: Revision,
    ) -> Result<Option<AssetText>, AssetReadError> {
        self.registry
            .read_asset_text_if_changed(self.fs, location, since)
    }
}

/// Context for [`super::NodeRuntime::produce`] and [`super::NodeRuntime::consume`].
///
/// Demand-style reads go through [`TickResolver`] (typically [`crate::dataflow::resolver::SessionHostResolver`]).
pub struct TickContext<'r> {
    node_id: NodeId,
    revision: Revision,
    resolver: &'r mut dyn TickResolver,
    slot_shapes: &'r SlotShapeRegistry,
    graphics: Option<Arc<dyn LpGraphics>>,
    time_provider: Option<Rc<dyn TimeProvider>>,
    button_service: Option<Rc<dyn ButtonService>>,
    radio_service: Option<Rc<dyn RadioService>>,
    frame_time_seconds: f32,
    /// Frame time the project's continuous fault began at, as derived at the
    /// END of the previous tick (`Engine::project_fault`). `None` = no node
    /// is in `Fault`. Read by outputs to decide whether to paint the fault
    /// pattern; the one-frame lag is why this can be a plain value rather
    /// than a query into a tree that is mid-walk.
    project_fault_since_seconds: Option<f32>,
    /// How many nodes were in `Fault` at that derivation — the N in the
    /// output's own "showing fault pattern" status.
    project_fault_node_count: u32,
    fault_presentation: FaultPresentation,
}

impl<'r> TickContext<'r> {
    pub fn new(
        node_id: NodeId,
        frame_id: Revision,
        resolver: &'r mut dyn TickResolver,
        slot_shapes: &'r SlotShapeRegistry,
    ) -> Self {
        Self::with_render_services(node_id, frame_id, resolver, slot_shapes, None, None, 0.0)
    }

    /// [`TickContext`] with graphics and frame time.
    pub fn with_render_services(
        node_id: NodeId,
        frame_id: Revision,
        resolver: &'r mut dyn TickResolver,
        slot_shapes: &'r SlotShapeRegistry,
        graphics: Option<Arc<dyn LpGraphics>>,
        time_provider: Option<Rc<dyn TimeProvider>>,
        frame_time_seconds: f32,
    ) -> Self {
        Self::with_engine_services(
            node_id,
            frame_id,
            resolver,
            slot_shapes,
            graphics,
            time_provider,
            None,
            None,
            frame_time_seconds,
        )
    }

    /// [`TickContext`] with graphics, time, and hardware input services.
    pub fn with_engine_services(
        node_id: NodeId,
        frame_id: Revision,
        resolver: &'r mut dyn TickResolver,
        slot_shapes: &'r SlotShapeRegistry,
        graphics: Option<Arc<dyn LpGraphics>>,
        time_provider: Option<Rc<dyn TimeProvider>>,
        button_service: Option<Rc<dyn ButtonService>>,
        radio_service: Option<Rc<dyn RadioService>>,
        frame_time_seconds: f32,
    ) -> Self {
        Self {
            node_id,
            revision: frame_id,
            resolver,
            slot_shapes,
            graphics,
            time_provider,
            button_service,
            radio_service,
            frame_time_seconds,
            project_fault_since_seconds: None,
            project_fault_node_count: 0,
            fault_presentation: FaultPresentation::default(),
        }
    }

    /// Attach the previous tick's project-fault verdict and the engine's
    /// presentation knob.
    ///
    /// A builder step rather than a constructor argument because almost no
    /// caller cares: only [`crate::nodes::OutputNode`] reads these, and
    /// every context built outside `Engine::tick_nodes` (tests, probes)
    /// honestly has no verdict to state.
    pub fn with_project_fault(
        mut self,
        since_seconds: Option<f32>,
        node_count: u32,
        presentation: FaultPresentation,
    ) -> Self {
        self.project_fault_since_seconds = since_seconds;
        self.project_fault_node_count = node_count;
        self.fault_presentation = presentation;
        self
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Frame time the project's continuous fault began at, or `None` when
    /// no node is in `Fault`. Compare against [`Self::time_seconds`].
    pub fn project_fault_since_seconds(&self) -> Option<f32> {
        self.project_fault_since_seconds
    }

    /// How many nodes are in `Fault` (0 when [`Self::project_fault_since_seconds`] is `None`).
    pub fn project_fault_node_count(&self) -> u32 {
        self.project_fault_node_count
    }

    /// What this engine wants outputs to do while the project is faulted.
    pub fn fault_presentation(&self) -> FaultPresentation {
        self.fault_presentation
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    /// Resolve a [`QueryKey`] for this frame (cache, bindings, optional host production).
    pub fn resolve(&mut self, query: &QueryKey) -> Result<Production, ResolveError> {
        self.resolver.resolve(query)
    }

    /// Resolve one of this node's consumed slots named by a constant path.
    ///
    /// Prefer this to building a [`QueryKey`] from a parsed path when the path
    /// is a literal: the parse and the key are memoized for as long as they
    /// stay valid, instead of being rebuilt and dropped every frame.
    pub fn resolve_static_consumed(
        &mut self,
        path: &'static str,
    ) -> Result<Production, ResolveError> {
        let node = self.node_id;
        self.resolver.resolve_static_consumed(node, path)
    }

    /// Resolve a bus channel named by a constant, for this node's read
    /// scope or another.
    ///
    /// Prefer this to building a [`QueryKey::Bus`] when the channel name is
    /// a literal: the [`lpc_model::ChannelName`] is built once per
    /// (scope, name) per structural epoch instead of per read.
    pub fn resolve_static_bus(
        &mut self,
        scope: Option<ScopeRef>,
        channel: &'static str,
    ) -> Result<Production, ResolveError> {
        self.resolver.resolve_static_bus(scope, channel)
    }

    /// The shared, interned form of `query` — for a node that keeps the keys
    /// it reads every tick instead of rebuilding them.
    ///
    /// The returned key stays valid forever (a [`QueryKey`] is not epoch
    /// scoped, unlike a `QueryId`); what [`Self::structure_epoch`] is for is
    /// noticing that the intern table dropped its half, so the holder can
    /// re-share rather than keep a private copy alive.
    pub fn intern_key(&mut self, query: &QueryKey) -> Rc<QueryKey> {
        self.resolver.intern_key(query)
    }

    /// How many times the graph has changed shape. Only equality across two
    /// observations is meaningful.
    pub fn structure_epoch(&self) -> u64 {
        self.resolver.structure_epoch()
    }

    /// Publish one of this node's runtime state slots for the current frame.
    ///
    /// `slot` is borrowed: a node publishes the same path every frame, so
    /// the caller keeps it and the provenance handle is interned
    /// ([`crate::dataflow::resolver::TickResolver::produced_slot_path`])
    /// rather than deep-copied per publish.
    pub fn publish_runtime_slot(
        &mut self,
        state_root: &dyn SlotAccess,
        slot: &SlotPath,
    ) -> Result<(), NodeError> {
        let (data, shape) = lookup_slot_data_and_shape(state_root, self.slot_shapes, slot)
            .map_err(|e| NodeError::msg(alloc::format!("runtime slot lookup {slot}: {e}")))?;
        let snapshot = lpc_wire::snapshot_slot_shape(shape, data, self.slot_shapes);
        let path = self.resolver.produced_slot_path(slot);
        let production = Production::new(
            snapshot,
            ProductionSource::ProducedSlot {
                node: self.node_id,
                slot: path,
            },
        );
        self.resolver
            .publish_produced_slot(self.node_id, slot.clone(), production)
            .map_err(|e| NodeError::msg(alloc::format!("publish runtime slot: {}", e.message)))
    }

    /// Resolve one of this node's consumed slots and parse it as a typed model value.
    pub fn resolve_consumed_slot_value<T>(&mut self, slot: &SlotPath) -> Result<T, NodeError>
    where
        T: FromLpValue,
    {
        let production = self
            .resolve(&QueryKey::ConsumedSlot {
                node: self.node_id,
                slot: slot.clone(),
            })
            .map_err(|e| NodeError::msg(alloc::format!("resolve consumed slot {slot}: {e:?}")))?;
        let value = production
            .value_leaf()
            .ok_or_else(|| NodeError::msg("resolved slot is not a value"))?;
        T::from_lp_value(value.value()).map_err(|e| {
            NodeError::msg(alloc::format!(
                "consumed slot {slot} has incompatible value: {e}"
            ))
        })
    }

    /// Resolve one of this node's consumed slots through a compiled accessor.
    pub fn resolve_consumed_slot_accessor_value<T>(
        &mut self,
        accessor: &SlotAccessor,
    ) -> Result<T, NodeError>
    where
        T: FromLpValue,
    {
        let production = self
            .resolve(&QueryKey::ConsumedSlotAccessor {
                node: self.node_id,
                accessor: accessor.clone(),
            })
            .map_err(|e| {
                NodeError::msg(alloc::format!(
                    "resolve consumed slot {}: {e:?}",
                    accessor.path()
                ))
            })?;
        let value = production
            .value_leaf()
            .ok_or_else(|| NodeError::msg("resolved slot is not a value"))?;
        T::from_lp_value(value.value()).map_err(|e| {
            NodeError::msg(alloc::format!(
                "consumed slot {} has incompatible value: {e}",
                accessor.path()
            ))
        })
    }

    pub fn slot_shapes(&self) -> &SlotShapeRegistry {
        self.slot_shapes
    }

    /// Monotonic shader time in seconds for the current engine frame.
    pub fn time_seconds(&self) -> f32 {
        self.frame_time_seconds
    }

    /// Graphics backend for shader compile and output buffers, when the engine has one installed.
    pub fn graphics(&self) -> Option<&dyn LpGraphics> {
        self.graphics.as_ref().map(|g| g.as_ref())
    }

    pub fn now_ms(&self) -> Option<u64> {
        self.time_provider
            .as_ref()
            .map(|provider| provider.now_ms())
    }

    pub fn elapsed_ms(&self, start_ms: u64) -> Option<u64> {
        self.time_provider
            .as_ref()
            .map(|provider| provider.elapsed_ms(start_ms))
    }

    pub fn button_service(&self) -> Option<Rc<dyn ButtonService>> {
        self.button_service.clone()
    }

    pub fn radio_service(&self) -> Option<Rc<dyn RadioService>> {
        self.radio_service.clone()
    }

    /// Materializes a visual product into a full texture through the active engine session.
    pub fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, NodeError> {
        self.resolver
            .render_texture(product, request)
            .map_err(|e| NodeError::msg(alloc::format!("render texture: {}", e.message)))
    }

    /// Renders a control product into an output-owned target through the active engine session.
    pub fn render_control(
        &mut self,
        product: ControlProduct,
        request: &ControlRenderRequest,
        target: ControlRenderTarget<'_>,
    ) -> Result<ControlLayout, NodeError> {
        self.resolver
            .render_control(product, request, target)
            .map_err(|e| NodeError::msg(alloc::format!("render control: {}", e.message)))
    }

    /// Where a control product's producer says its lamps land on THIS
    /// node's wire: the producer's resolved runs, filtered to the ones
    /// addressed to the current (output) node — `None` for auto-flow,
    /// `Some(vec![])` for "patched, nothing lands here" (D40; the
    /// distinction is documented on the engine's implementation).
    ///
    /// The output asks this once per producer per frame, between resolving
    /// its input (which ticks the producer) and rendering.
    pub fn control_patch_placement(
        &self,
        product: ControlProduct,
    ) -> Option<alloc::vec::Vec<PatchedRun>> {
        self.resolver.control_patch_placement(product, self.node_id)
    }

    /// Register this (output) node's authored name for patch routing —
    /// called at the top of the output's `consume`, the one place its def
    /// is readable. Returns the colliding name when a live sibling already
    /// claims it (surface it as runtime status; routing stays exact-match).
    pub fn register_output_identity(
        &mut self,
        name: Option<alloc::string::String>,
    ) -> Option<alloc::string::String> {
        let node = self.node_id;
        let revision = self.revision;
        self.resolver.register_output_identity(node, name, revision)
    }

    /// Every registered output name plus the revision the set last changed
    /// at — the fixture's dangling-entry check caches on the revision.
    pub fn known_output_names(&self) -> (alloc::vec::Vec<alloc::string::String>, Revision) {
        self.resolver.known_output_names()
    }

    /// Publishes this node's timebase for the current tick.
    ///
    /// A node that produces a [`lpc_model::TimeProduct`] calls this from its
    /// `produce` so that everything holding the handle can be answered from
    /// the engine's timebase store instead of by dispatching back into the
    /// node. `effective_seconds` is the timebase's own notion of now (not
    /// [`Self::time_seconds`], which stays raw engine wall clock);
    /// `delta_seconds` is what it advanced this tick, and may be negative
    /// when a device scrubs backwards.
    pub fn publish_timebase(&mut self, effective_seconds: f32, delta_seconds: f32) {
        let node = self.node_id;
        let revision = self.revision;
        self.resolver
            .publish_timebase(node, effective_seconds, delta_seconds, revision);
    }

    /// The bus scope this node reads from — the scope half of a scoped
    /// [`QueryKey::Bus`](crate::dataflow::resolver::QueryKey) key. `None` on
    /// hosts with no scope model.
    pub fn bus_read_scope(&self) -> Option<ScopeRef> {
        self.resolver.node_scope(self.node_id)
    }

    /// The channel + writer scope supplying one of this node's consumed
    /// slots, when a bus channel with a live writer is what supplies it —
    /// the provenance a phasor identity is derived from (parent D3).
    pub fn consumed_slot_bus_provenance(
        &self,
        slot: &SlotPath,
    ) -> Option<(ScopeRef, lpc_model::ChannelName)> {
        self.resolver
            .consumed_slot_bus_provenance(self.node_id, slot)
    }

    /// The effective seconds behind a time product.
    pub fn time_product_seconds(&self, product: TimeProduct) -> Result<f32, NodeError> {
        self.resolver
            .time_product_seconds(product)
            .map_err(|e| NodeError::msg(alloc::format!("time product seconds: {}", e.message)))
    }

    /// How far a time product advanced during the current tick.
    pub fn time_product_delta(&self, product: TimeProduct) -> Result<f32, NodeError> {
        self.resolver
            .time_product_delta(product)
            .map_err(|e| NodeError::msg(alloc::format!("time product delta: {}", e.message)))
    }

    /// A time product's wrapped `[0,1)` cycle position and completed-cycle
    /// count, under `config`, for the integrator named by `key`.
    ///
    /// Tick-side: the first call in a tick advances the phasor, later calls
    /// in the same tick see the same values. `key` is provenance-derived
    /// (where the config came from), never caller-invented — that is what
    /// makes two consumers of one channel-driven config share a phase while
    /// two slot-local configs stay independent.
    ///
    /// The result is the RAW ramp: [`PhasorConfig::waveform`] and
    /// `phase_offset` are the caller's to apply.
    ///
    /// `reader` names who is asking — this node and the consumed slot the
    /// config was resolved for. The store records it (with the config's
    /// shaping) as witness data for the timebase probe; it never affects
    /// the answer.
    pub fn time_product_phasor(
        &mut self,
        product: TimeProduct,
        key: &PhasorKey,
        config: &PhasorConfig,
        reader: (NodeId, &SlotPath),
    ) -> Result<(f32, u32), NodeError> {
        self.resolver
            .time_product_phasor(product, key, config, reader)
            .map_err(|e| NodeError::msg(alloc::format!("time product phasor: {}", e.message)))
    }

    /// Mutates a single existing runtime buffer in place and marks it changed for `frame`.
    pub fn with_runtime_buffer_mut<F>(
        &mut self,
        id: RuntimeBufferId,
        frame: Revision,
        write: F,
    ) -> Result<(), NodeError>
    where
        F: FnOnce(&mut RuntimeBuffer) -> Result<(), NodeError>,
    {
        let buffer = self
            .resolver
            .runtime_buffer_mut(id, frame)
            .map_err(|e| NodeError::msg(alloc::format!("runtime buffer mut: {}", e.message)))?;
        write(buffer)
    }
}

impl lpc_model::SlotReadContext for TickContext<'_> {
    type Error = NodeError;

    fn read_slot_value<T>(&mut self, accessor: &SlotAccessor) -> Result<T, Self::Error>
    where
        T: FromLpValue,
    {
        self.resolve_consumed_slot_accessor_value(accessor)
    }

    fn is_optional_none_error(error: &Self::Error) -> bool {
        match error {
            NodeError::Message(message) => message.contains("option slot is none"),
        }
    }
}

/// Context passed to [`super::ControlNode`] materialization hooks.
pub struct ControlRenderContext<'a> {
    node_id: NodeId,
    revision: Revision,
    graphics: Option<Arc<dyn LpGraphics>>,
    frame_time_seconds: f32,
    /// Device-level safe-mode output ceiling, Q16. See
    /// `Engine::set_safe_output_clamp` — device state, never project data.
    safe_output_clamp_q16: Option<u32>,
    services: &'a mut dyn ControlRenderServices,
}

impl<'a> ControlRenderContext<'a> {
    pub fn new(
        node_id: NodeId,
        revision: Revision,
        graphics: Option<Arc<dyn LpGraphics>>,
        frame_time_seconds: f32,
        safe_output_clamp_q16: Option<u32>,
        services: &'a mut dyn ControlRenderServices,
    ) -> Self {
        Self {
            node_id,
            revision,
            graphics,
            frame_time_seconds,
            safe_output_clamp_q16,
            services,
        }
    }

    /// The device-level safe-mode ceiling, when one is armed (Q16).
    pub fn safe_output_clamp_q16(&self) -> Option<u32> {
        self.safe_output_clamp_q16
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn graphics(&self) -> Option<&dyn LpGraphics> {
        self.graphics.as_ref().map(|g| g.as_ref())
    }

    pub fn time_seconds(&self) -> f32 {
        self.frame_time_seconds
    }

    /// The space the bound visual product lives in — ask before choosing
    /// which of your own coordinate sets to send (plan D17).
    pub fn visual_product_space(
        &mut self,
        product: VisualProduct,
    ) -> Result<ProductSpaceInfo, NodeError> {
        self.services.visual_product_space(product)
    }

    pub fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, NodeError> {
        self.services.render_texture(product, request)
    }

    pub fn render_texture_into(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        target: &mut TextureHandle,
    ) -> Result<(), NodeError> {
        self.services.render_texture_into(product, request, target)
    }

    pub fn sample_visual_into(
        &mut self,
        product: VisualProduct,
        request: VisualSampleBufferRequest<'_>,
        target: VisualSampleTarget<'_>,
    ) -> Result<(), NodeError> {
        self.services.sample_visual_into(product, request, target)
    }

    /// The effective seconds behind a time product.
    pub fn time_product_seconds(&self, product: TimeProduct) -> Result<f32, NodeError> {
        self.services.time_product_seconds(product)
    }

    /// How far a time product advanced during the most recent tick.
    pub fn time_product_delta(&self, product: TimeProduct) -> Result<f32, NodeError> {
        self.services.time_product_delta(product)
    }

    /// A phasor's raw ramp as the last tick left it — see [`TimebaseRead`]
    /// for why render never advances one.
    pub fn time_product_phasor_read(
        &self,
        product: TimeProduct,
        key: &PhasorKey,
    ) -> Result<(f32, u32), NodeError> {
        self.services.time_product_phasor_read(product, key)
    }
}

/// Read-only timebase access shared by both render-phase service traits.
///
/// Render is not a tick: `render_texture_into`/`sample_visual_into` can run
/// more than once per tick and can run outside a tick entirely (probes,
/// preview surfaces). A phasor that advanced or materialized here would tie
/// its rate to how many previews happen to be open, so the render phase only
/// ever *reads* — an unmaterialized phasor reads as the start of its first
/// cycle rather than being born.
///
/// Defaulted throughout so node-level test fakes keep compiling.
pub trait TimebaseRead {
    fn time_product_seconds(&self, product: TimeProduct) -> Result<f32, NodeError> {
        let _ = product;
        Err(NodeError::msg("render context has no timebase access"))
    }

    fn time_product_delta(&self, product: TimeProduct) -> Result<f32, NodeError> {
        let _ = product;
        Err(NodeError::msg("render context has no timebase access"))
    }

    /// The phasor's current raw ramp, without materializing or advancing it.
    fn time_product_phasor_read(
        &self,
        product: TimeProduct,
        key: &PhasorKey,
    ) -> Result<(f32, u32), NodeError> {
        let _ = (product, key);
        Err(NodeError::msg("render context has no timebase access"))
    }
}

/// Services available while materializing a [`crate::products::control::ControlProduct`].
pub trait ControlRenderServices: TimebaseRead {
    /// The space a visual product lives in (plan D17): a metadata query
    /// routed exactly like `sample_visual_into`, so the product wire value
    /// stays `{node, output}`.
    ///
    /// Defaulted to 2D-with-no-opinion so node-level test fakes keep
    /// compiling; the engine host overrides it.
    fn visual_product_space(
        &mut self,
        product: VisualProduct,
    ) -> Result<ProductSpaceInfo, NodeError> {
        let _ = product;
        Ok(ProductSpaceInfo::two_d())
    }

    fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, NodeError>;

    fn render_texture_into(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        target: &mut TextureHandle,
    ) -> Result<(), NodeError>;

    fn sample_visual_into(
        &mut self,
        product: VisualProduct,
        request: VisualSampleBufferRequest<'_>,
        target: VisualSampleTarget<'_>,
    ) -> Result<(), NodeError>;
}

/// Services available while materializing a [`crate::products::visual::VisualProduct`].
pub trait VisualRenderServices: TimebaseRead {
    /// The space a visual product lives in (plan D17): a metadata query
    /// routed exactly like `sample_visual_into`, so the product wire value
    /// stays `{node, output}`.
    ///
    /// Defaulted to 2D-with-no-opinion so node-level test fakes keep
    /// compiling; the engine host overrides it.
    fn visual_product_space(
        &mut self,
        product: VisualProduct,
    ) -> Result<ProductSpaceInfo, NodeError> {
        let _ = product;
        Ok(ProductSpaceInfo::two_d())
    }

    fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, NodeError>;

    fn render_texture_into(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        target: &mut TextureHandle,
    ) -> Result<(), NodeError>;

    fn sample_visual_into(
        &mut self,
        product: VisualProduct,
        request: VisualSampleBufferRequest<'_>,
        target: VisualSampleTarget<'_>,
    ) -> Result<(), NodeError>;
}

/// Context passed to [`super::RenderNode`] materialization hooks.
pub struct RenderContext<'a> {
    node_id: NodeId,
    revision: Revision,
    graphics: Option<Arc<dyn LpGraphics>>,
    time_provider: Option<Rc<dyn TimeProvider>>,
    frame_time_seconds: f32,
    services: Option<&'a mut dyn VisualRenderServices>,
}

impl<'a> RenderContext<'a> {
    pub fn new(
        node_id: NodeId,
        revision: Revision,
        graphics: Option<Arc<dyn LpGraphics>>,
        time_provider: Option<Rc<dyn TimeProvider>>,
        frame_time_seconds: f32,
    ) -> Self {
        Self {
            node_id,
            revision,
            graphics,
            time_provider,
            frame_time_seconds,
            services: None,
        }
    }

    pub fn with_services(
        node_id: NodeId,
        revision: Revision,
        graphics: Option<Arc<dyn LpGraphics>>,
        time_provider: Option<Rc<dyn TimeProvider>>,
        frame_time_seconds: f32,
        services: &'a mut dyn VisualRenderServices,
    ) -> Self {
        Self {
            node_id,
            revision,
            graphics,
            time_provider,
            frame_time_seconds,
            services: Some(services),
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn graphics(&self) -> Option<&dyn LpGraphics> {
        self.graphics.as_ref().map(|g| g.as_ref())
    }

    pub fn now_ms(&self) -> Option<u64> {
        self.time_provider
            .as_ref()
            .map(|provider| provider.now_ms())
    }

    pub fn elapsed_ms(&self, start_ms: u64) -> Option<u64> {
        self.time_provider
            .as_ref()
            .map(|provider| provider.elapsed_ms(start_ms))
    }

    pub fn time_seconds(&self) -> f32 {
        self.frame_time_seconds
    }

    /// The space an upstream visual product lives in — forwarded by nodes
    /// that pass a product through (playlist, module).
    pub fn visual_product_space(
        &mut self,
        product: VisualProduct,
    ) -> Result<ProductSpaceInfo, NodeError> {
        self.services
            .as_mut()
            .ok_or_else(|| NodeError::msg("render context has no visual render services"))?
            .visual_product_space(product)
    }

    pub fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, NodeError> {
        self.services
            .as_mut()
            .ok_or_else(|| NodeError::msg("render context has no visual render services"))?
            .render_texture(product, request)
    }

    pub fn render_texture_into(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        target: &mut TextureHandle,
    ) -> Result<(), NodeError> {
        self.services
            .as_mut()
            .ok_or_else(|| NodeError::msg("render context has no visual render services"))?
            .render_texture_into(product, request, target)
    }

    pub fn sample_visual_into(
        &mut self,
        product: VisualProduct,
        request: VisualSampleBufferRequest<'_>,
        target: VisualSampleTarget<'_>,
    ) -> Result<(), NodeError> {
        self.services
            .as_mut()
            .ok_or_else(|| NodeError::msg("render context has no visual render services"))?
            .sample_visual_into(product, request, target)
    }

    /// The effective seconds behind a time product.
    pub fn time_product_seconds(&self, product: TimeProduct) -> Result<f32, NodeError> {
        self.timebase()?.time_product_seconds(product)
    }

    /// How far a time product advanced during the most recent tick.
    pub fn time_product_delta(&self, product: TimeProduct) -> Result<f32, NodeError> {
        self.timebase()?.time_product_delta(product)
    }

    /// A phasor's raw ramp as the last tick left it — see [`TimebaseRead`]
    /// for why render never advances one.
    pub fn time_product_phasor_read(
        &self,
        product: TimeProduct,
        key: &PhasorKey,
    ) -> Result<(f32, u32), NodeError> {
        self.timebase()?.time_product_phasor_read(product, key)
    }

    fn timebase(&self) -> Result<&dyn VisualRenderServices, NodeError> {
        self.services
            .as_ref()
            .map(|services| &**services)
            .ok_or_else(|| NodeError::msg("render context has no visual render services"))
    }
}

/// Context for [`super::Node::destroy`](super::NodeRuntime::destroy).
pub struct DestroyCtx {
    node_id: NodeId,
    revision: Revision,
}

impl DestroyCtx {
    /// Create a new destroy context.
    pub fn new(node_id: NodeId, frame_id: Revision) -> Self {
        Self {
            node_id,
            revision: frame_id,
        }
    }

    /// Node being destroyed.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Frame at which destruction is occurring.
    pub fn frame_id(&self) -> Revision {
        self.revision
    }
}

/// Context for [`super::Node::handle_memory_pressure`](super::NodeRuntime::handle_memory_pressure).
pub struct MemPressureCtx {
    node_id: NodeId,
    revision: Revision,
}

impl MemPressureCtx {
    /// Create a new memory pressure context.
    pub fn new(node_id: NodeId, frame_id: Revision) -> Self {
        Self {
            node_id,
            revision: frame_id,
        }
    }

    /// Node under pressure.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Current frame.
    pub fn revision(&self) -> Revision {
        self.revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataflow::binding::{
        BindingDraft, BindingEntry, BindingPriority, BindingRef, BindingSource, BindingTarget,
    };
    use crate::dataflow::resolver::resolve_trace::ResolveLogLevel;
    use crate::dataflow::resolver::{
        Production, QueryKey, ResolveHost, ResolveSession, ResolveTrace, Resolver,
        SessionHostResolver, TickResolver,
    };
    use crate::node::{NodeRuntime, RuntimeStateShape};
    use alloc::string::String;
    use alloc::vec::Vec;
    use lpc_model::{Kind, LpValue, SlotPath, SlotShapeRegistry, Slotted, ValueSlot};
    use lps_shared::LpsValueF32;

    #[derive(Default, Slotted)]
    #[slot(default_role = "state")]
    struct TestRuntimeState {
        #[slot(produced)]
        pub value: ValueSlot<f32>,
    }

    #[derive(Default)]
    struct TestBindings {
        entries: Vec<(BindingRef, BindingEntry)>,
    }

    impl TestBindings {
        fn add(&mut self, draft: BindingDraft, revision: Revision) {
            let binding_ref = BindingRef::new(draft.owner, self.entries.len());
            self.entries.push((
                binding_ref,
                BindingEntry {
                    source: draft.source,
                    target: draft.target,
                    priority: draft.priority,
                    kind: draft.kind,
                    version: revision,
                    owner: draft.owner,
                },
            ));
        }

        fn binding_for_consumed_slot(
            &self,
            node: NodeId,
            slot: &SlotPath,
        ) -> Option<(BindingRef, BindingEntry)> {
            self.entries.iter().find_map(|(binding_ref, entry)| {
                matches!(
                    &entry.target,
                    BindingTarget::ConsumedSlot { node: n, slot: p } if *n == node && p == slot
                )
                .then(|| (*binding_ref, entry.clone()))
            })
        }

        fn providers_for_bus(
            &self,
            _scope: Option<crate::node::ScopeRef>,
            channel: &lpc_model::ChannelName,
        ) -> Vec<(BindingRef, BindingEntry)> {
            self.entries
                .iter()
                .filter_map(|(binding_ref, entry)| {
                    matches!(&entry.target, BindingTarget::BusChannel(c) if c == channel)
                        .then(|| (*binding_ref, entry.clone()))
                })
                .collect()
        }
    }

    #[derive(Default)]
    struct PanicProduceHost {
        bindings: TestBindings,
    }

    impl ResolveHost for PanicProduceHost {
        fn produce(
            &mut self,
            _query: &QueryKey,
            _session: &mut ResolveSession<'_>,
        ) -> Result<Production, crate::dataflow::resolver::SessionResolveError> {
            Err(crate::dataflow::resolver::SessionResolveError::other(
                "unexpected produce in TickContext test",
            ))
        }

        fn binding_for_consumed_slot(
            &self,
            node: NodeId,
            slot: &SlotPath,
        ) -> Option<(BindingRef, BindingEntry)> {
            self.bindings.binding_for_consumed_slot(node, slot)
        }

        fn providers_for_bus(
            &self,
            scope: Option<crate::node::ScopeRef>,
            channel: &lpc_model::ChannelName,
        ) -> Vec<(BindingRef, BindingEntry)> {
            self.bindings.providers_for_bus(scope, channel)
        }
    }

    fn session_bundle(resolver: &mut Resolver, frame: Revision) -> ResolveSession<'_> {
        ResolveSession::new(frame, resolver, ResolveTrace::new(ResolveLogLevel::Off))
    }

    #[test]
    fn tick_context_accessors() {
        let mut resolver = Resolver::new();
        let frame = Revision::new(10);
        let mut session = session_bundle(&mut resolver, frame);
        let mut host = PanicProduceHost::default();
        let slot_shapes = SlotShapeRegistry::default();

        let mut bridge = SessionHostResolver {
            session: &mut session,
            host: &mut host,
        };
        let ctx = TickContext::new(
            NodeId::new(7),
            Revision::new(3),
            &mut bridge as &mut dyn TickResolver,
            &slot_shapes,
        );

        assert_eq!(ctx.node_id(), NodeId::new(7));
        assert_eq!(ctx.revision(), Revision::new(3));
    }

    #[test]
    fn tick_context_resolve_bus_query() {
        let mut bindings = TestBindings::default();
        let frame = Revision::new(10);
        let channel = lpc_model::ChannelName(String::from("level_bus"));
        bindings.add(
            BindingDraft {
                source: BindingSource::Literal(lpc_model::LpValue::F32(7.8)),
                target: BindingTarget::BusChannel(channel.clone()),
                priority: BindingPriority::new(0),
                kind: lpc_model::Kind::Amplitude,
                owner: NodeId::new(1),
            },
            frame,
        );

        let mut resolver = Resolver::new();
        let mut session = session_bundle(&mut resolver, frame);
        let mut host = PanicProduceHost { bindings };
        let slot_shapes = SlotShapeRegistry::default();
        let mut bridge = SessionHostResolver {
            session: &mut session,
            host: &mut host,
        };
        let mut ctx = TickContext::new(
            NodeId::new(1),
            frame,
            &mut bridge as &mut dyn TickResolver,
            &slot_shapes,
        );
        let pv = ctx
            .resolve(&QueryKey::Bus {
                scope: None,
                channel: channel.clone(),
            })
            .expect("resolve bus");
        assert!(pv.as_value().expect("value").eq(&LpsValueF32::F32(7.8)));
    }

    #[test]
    fn tick_context_resolve_consumed_slot_query() {
        let mut bindings = TestBindings::default();
        let frame = Revision::new(10);
        let node = NodeId::new(3);
        let input = SlotPath::parse("in").unwrap();
        bindings.add(
            BindingDraft {
                source: BindingSource::Literal(lpc_model::LpValue::F32(4.25)),
                target: BindingTarget::ConsumedSlot {
                    node,
                    slot: input.clone(),
                },
                priority: BindingPriority::new(0),
                kind: Kind::Amplitude,
                owner: node,
            },
            frame,
        );

        let mut resolver = Resolver::new();
        let mut session = session_bundle(&mut resolver, frame);
        let mut host = PanicProduceHost { bindings };
        let slot_shapes = SlotShapeRegistry::default();
        let mut bridge = SessionHostResolver {
            session: &mut session,
            host: &mut host,
        };
        let mut ctx = TickContext::new(
            node,
            frame,
            &mut bridge as &mut dyn TickResolver,
            &slot_shapes,
        );

        let pv = ctx
            .resolve(&QueryKey::ConsumedSlot {
                node,
                slot: input.clone(),
            })
            .expect("resolve consumed slot");
        assert!(pv.as_value().expect("value").eq(&LpsValueF32::F32(4.25)));
    }

    #[test]
    fn tick_context_publish_runtime_slot_satisfies_same_frame_cache() {
        let node = NodeId::new(7);
        let frame = Revision::new(10);
        let mut resolver = Resolver::new();
        let mut session = session_bundle(&mut resolver, frame);
        let mut host = PanicProduceHost::default();
        let mut slot_shapes = SlotShapeRegistry::default();
        TestRuntimeState::register_runtime_state_shape(&mut slot_shapes).expect("state shape");
        let mut bridge = SessionHostResolver {
            session: &mut session,
            host: &mut host,
        };
        let mut ctx = TickContext::new(
            node,
            frame,
            &mut bridge as &mut dyn TickResolver,
            &slot_shapes,
        );
        let state = TestRuntimeState {
            value: ValueSlot::new(3.5),
        };
        let slot = SlotPath::parse("value").expect("value slot");

        ctx.publish_runtime_slot(&state, &slot).expect("publish");
        let production = ctx
            .resolve(&QueryKey::ProducedSlot { node, slot })
            .expect("resolve published slot");

        assert_eq!(
            *production.value_leaf().expect("leaf").value(),
            LpValue::F32(3.5)
        );
    }

    struct FixtureProduceHost {
        node: NodeId,
        out_path: SlotPath,
    }

    impl ResolveHost for FixtureProduceHost {
        fn produce(
            &mut self,
            query: &QueryKey,
            session: &mut ResolveSession<'_>,
        ) -> Result<Production, crate::dataflow::resolver::SessionResolveError> {
            match query {
                QueryKey::ConsumedSlot { node, slot }
                    if *node == self.node && *slot == self.out_path =>
                {
                    Ok(Production::value(
                        lpc_model::WithRevision::new(session.revision(), LpsValueF32::F32(11.0)),
                        crate::dataflow::resolver::ProductionSource::Default,
                    )?)
                }
                _ => Err(crate::dataflow::resolver::SessionResolveError::other(
                    "fixture produce mismatch",
                )),
            }
        }
    }

    /// Dummy node that uses [`TickContext::resolve`](TickContext::resolve) from [`super::super::NodeRuntime::produce`].
    struct QueryResolvingNode {
        query: QueryKey,
        resolved_value: Option<f32>,
    }

    impl super::super::NodeRuntime for QueryResolvingNode {
        fn produce(
            &mut self,
            _slot: &SlotPath,
            ctx: &mut TickContext<'_>,
        ) -> Result<super::super::ProduceResult, crate::node::NodeError> {
            let pv = ctx.resolve(&self.query).map_err(|e| {
                crate::node::NodeError::msg(alloc::format!("resolve failed: {}", e.message))
            })?;
            if let LpsValueF32::F32(v) = pv.as_value().expect("value") {
                self.resolved_value = Some(v);
            }
            Ok(super::super::ProduceResult::Produced)
        }

        fn destroy(&mut self, _ctx: &mut super::DestroyCtx) -> Result<(), crate::node::NodeError> {
            Ok(())
        }

        fn handle_memory_pressure(
            &mut self,
            _level: super::super::PressureLevel,
            _ctx: &mut super::MemPressureCtx,
        ) -> Result<(), crate::node::NodeError> {
            Ok(())
        }
    }

    #[test]
    fn dummy_node_can_resolve_bus_query_from_produce() {
        let mut bindings = TestBindings::default();
        let frame = Revision::new(10);
        let channel = lpc_model::ChannelName(String::from("in"));
        bindings.add(
            BindingDraft {
                source: BindingSource::Literal(lpc_model::LpValue::F32(8.8)),
                target: BindingTarget::BusChannel(channel.clone()),
                priority: BindingPriority::new(0),
                kind: Kind::Amplitude,
                owner: NodeId::new(2),
            },
            frame,
        );

        let mut resolver = Resolver::new();
        let mut session = session_bundle(&mut resolver, frame);
        let mut host = PanicProduceHost { bindings };
        let slot_shapes = SlotShapeRegistry::default();

        let mut node = QueryResolvingNode {
            query: QueryKey::Bus {
                scope: None,
                channel,
            },
            resolved_value: None,
        };

        let mut bridge = SessionHostResolver {
            session: &mut session,
            host: &mut host,
        };
        let mut ctx = TickContext::new(
            NodeId::new(2),
            frame,
            &mut bridge as &mut dyn TickResolver,
            &slot_shapes,
        );

        node.produce(&SlotPath::root(), &mut ctx)
            .expect("produce should succeed");
        assert_eq!(node.resolved_value, Some(8.8));
    }

    #[test]
    fn dummy_node_can_resolve_consumed_slot_via_host_from_produce() {
        let frame = Revision::new(10);
        let node_id = NodeId::new(2);
        let input_path = SlotPath::parse("fixture_in").unwrap();

        let mut resolver = Resolver::new();
        let mut session = session_bundle(&mut resolver, frame);
        let mut host = FixtureProduceHost {
            node: node_id,
            out_path: input_path.clone(),
        };
        let slot_shapes = SlotShapeRegistry::default();

        let mut node = QueryResolvingNode {
            query: QueryKey::ConsumedSlot {
                node: node_id,
                slot: input_path,
            },
            resolved_value: None,
        };

        let mut bridge = SessionHostResolver {
            session: &mut session,
            host: &mut host,
        };
        let mut ctx = TickContext::new(
            node_id,
            frame,
            &mut bridge as &mut dyn TickResolver,
            &slot_shapes,
        );

        node.produce(&SlotPath::root(), &mut ctx)
            .expect("produce should succeed");
        assert_eq!(node.resolved_value, Some(11.0));
    }

    #[test]
    fn destroy_ctx_accessors() {
        let ctx = DestroyCtx::new(NodeId::new(1), Revision::new(99));
        assert_eq!(ctx.node_id(), NodeId::new(1));
        assert_eq!(ctx.frame_id(), Revision::new(99));
    }

    #[test]
    fn mem_pressure_ctx_accessors() {
        let ctx = MemPressureCtx::new(NodeId::new(2), Revision::new(100));
        assert_eq!(ctx.node_id(), NodeId::new(2));
        assert_eq!(ctx.revision(), Revision::new(100));
    }
}
