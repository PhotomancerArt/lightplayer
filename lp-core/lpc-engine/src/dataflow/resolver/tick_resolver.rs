//! Node-facing demand resolution facade ([`TickResolver`]) backed by session + host.

use alloc::vec::Vec;

use crate::dataflow::resolver::production::Production;
use crate::dataflow::resolver::query_key::QueryKey;
use crate::dataflow::resolver::resolve_error::{ResolveError, SessionResolveError};
use crate::dataflow::resolver::resolve_host::ResolveHost;
use crate::dataflow::resolver::resolve_session::ResolveSession;
use crate::dataflow::timebase::PhasorKey;
use crate::node::{PatchedRun, ScopeRef};
use crate::products::control::{
    ControlLayout, ControlProduct, ControlRenderRequest, ControlRenderTarget,
};
use crate::products::visual::{RenderTextureRequest, TextureRenderProduct, VisualProduct};
use crate::resource::{RuntimeBuffer, RuntimeBufferId};
use lpc_model::{ChannelName, NodeId, PhasorConfig, Revision, SlotPath, TimeProduct};

/// Narrow API for [`crate::node::TickContext`] demand reads (`QueryKey` → [`Production`]).
pub trait TickResolver {
    /// Resolve `query`. Borrowed rather than owned so a node that reads the
    /// same slot every frame can keep one key instead of rebuilding it.
    fn resolve(&mut self, query: &QueryKey) -> Result<Production, ResolveError>;

    /// Resolve one of `node`'s consumed slots named by a constant path,
    /// without rebuilding the query each frame.
    fn resolve_static_consumed(
        &mut self,
        node: NodeId,
        path: &'static str,
    ) -> Result<Production, ResolveError>;

    /// Resolve the bus `channel` in `scope`, named by a constant, without
    /// rebuilding the [`ChannelName`] each frame.
    ///
    /// Defaulted through [`Self::resolve`] so a node-level test fake need
    /// only answer the generic path; the session-backed implementation
    /// interns the key per structural epoch instead.
    fn resolve_static_bus(
        &mut self,
        scope: Option<ScopeRef>,
        channel: &'static str,
    ) -> Result<Production, ResolveError> {
        self.resolve(&QueryKey::Bus {
            scope,
            channel: ChannelName(alloc::string::String::from(channel)),
        })
    }

    /// The shared, interned form of `query`, for a caller that keeps a key
    /// across frames rather than rebuilding it.
    ///
    /// Defaulted to a private copy so a test fake needs no intern table; the
    /// session-backed implementation hands back the table's own `Rc`, which
    /// is what keeps a per-node key cache from doubling the resident path
    /// data the resolver already holds.
    fn intern_key(&mut self, query: &QueryKey) -> alloc::rc::Rc<QueryKey> {
        alloc::rc::Rc::new(query.clone())
    }

    /// How many times the graph has changed shape. Only equality across two
    /// observations is meaningful. A cache holding [`Self::intern_key`]
    /// results watches this to drop keys the intern table has forgotten;
    /// the default never moves, which is correct for a fake that never
    /// invalidates.
    fn structure_epoch(&self) -> u64 {
        0
    }

    /// The shared handle to use for a produced slot's path in a
    /// [`crate::dataflow::resolver::ProductionSource::ProducedSlot`].
    ///
    /// Defaulted to a private copy for test fakes; the session-backed
    /// implementation interns one handle per produced path so a per-frame
    /// publish copies a pointer instead of the path.
    fn produced_slot_path(&mut self, slot: &SlotPath) -> alloc::rc::Rc<SlotPath> {
        alloc::rc::Rc::new(slot.clone())
    }

    fn publish_produced_slot(
        &mut self,
        node: NodeId,
        slot: SlotPath,
        production: Production,
    ) -> Result<(), ResolveError>;

    /// The bus scope `node` reads from. `None` on hosts with no scope model
    /// (test fakes), where every read shares the unscoped key.
    fn node_scope(&self, node: NodeId) -> Option<ScopeRef> {
        let _ = node;
        None
    }

    /// The channel + writer scope behind a consumed slot's value, when a bus
    /// channel with a live writer is what supplies it. See
    /// [`crate::dataflow::resolver::ResolveHost::consumed_slot_bus_provenance`].
    fn consumed_slot_bus_provenance(
        &self,
        node: NodeId,
        slot: &SlotPath,
    ) -> Option<(ScopeRef, ChannelName)> {
        let _ = (node, slot);
        None
    }

    fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, ResolveError>;

    fn render_control(
        &mut self,
        product: ControlProduct,
        request: &ControlRenderRequest,
        target: ControlRenderTarget<'_>,
    ) -> Result<ControlLayout, ResolveError>;

    /// Where a control product's producer says its lamps land on
    /// `consumer`'s wire — the resolved patch runs addressed to that
    /// output — or `None` for auto-flow (`Some(vec![])` = patched, nothing
    /// here; see the engine implementation for the D40 contract).
    ///
    /// Defaulted to `None` so a node-level test fake need not model patching
    /// to tick a node that asks; the engine's own resolver forwards to the
    /// producing node.
    fn control_patch_placement(
        &self,
        product: ControlProduct,
        consumer: NodeId,
    ) -> Option<Vec<PatchedRun>> {
        let _ = (product, consumer);
        None
    }

    /// Record `node`'s authored output name for patch routing; answers a
    /// live sibling's colliding claim. Defaulted so fakes need not model
    /// output identity.
    fn register_output_identity(
        &mut self,
        node: NodeId,
        name: Option<alloc::string::String>,
        revision: Revision,
    ) -> Option<alloc::string::String> {
        let _ = (node, name, revision);
        None
    }

    /// The registered output-name set and the revision it last changed at.
    fn known_output_names(&self) -> (Vec<alloc::string::String>, Revision) {
        (Vec::new(), Revision::default())
    }

    fn runtime_buffer_mut(
        &mut self,
        id: RuntimeBufferId,
        frame: Revision,
    ) -> Result<&mut RuntimeBuffer, ResolveError>;

    /// Publish this node's timebase for the current tick.
    ///
    /// Defaulted (with the three reads below) so a node-level test fake does
    /// not have to model the timebase store to tick a node that publishes
    /// one; the engine's own resolver always forwards to the store.
    fn publish_timebase(
        &mut self,
        clock: NodeId,
        effective_seconds: f32,
        delta_seconds: f32,
        at: Revision,
    ) {
        let _ = (clock, effective_seconds, delta_seconds, at);
    }

    fn time_product_seconds(&self, product: TimeProduct) -> Result<f32, ResolveError> {
        let _ = product;
        Err(ResolveError::new(alloc::format!(
            "resolver has no timebase access"
        )))
    }

    fn time_product_delta(&self, product: TimeProduct) -> Result<f32, ResolveError> {
        let _ = product;
        Err(ResolveError::new(alloc::format!(
            "resolver has no timebase access"
        )))
    }

    fn time_product_phasor(
        &mut self,
        product: TimeProduct,
        key: &PhasorKey,
        config: &PhasorConfig,
        reader: (NodeId, &SlotPath),
    ) -> Result<(f32, u32), ResolveError> {
        let _ = (product, key, config, reader);
        Err(ResolveError::new(alloc::format!(
            "resolver has no timebase access"
        )))
    }
}

/// Bridges [`ResolveSession`] + [`ResolveHost`] into a [`TickResolver`].
///
/// `'resolver` is the session's resolver borrow ([`ResolveSession`]'s lifetime parameter).
/// `'sess` is the borrow of that session from the caller (often shorter); splitting them avoids
/// invariant `'sess == 'resolver` churn when constructing from `&mut ResolveSession<'resolver>`.
pub struct SessionHostResolver<'sess, 'resolver, 'host> {
    pub session: &'sess mut ResolveSession<'resolver>,
    pub host: &'host mut dyn ResolveHost,
}

impl<'sess, 'resolver, 'host> TickResolver for SessionHostResolver<'sess, 'resolver, 'host> {
    fn resolve(&mut self, query: &QueryKey) -> Result<Production, ResolveError> {
        self.session
            .resolve(self.host, query)
            .map_err(|e: SessionResolveError| ResolveError::new(alloc::format!("{e}")))
    }

    fn resolve_static_consumed(
        &mut self,
        node: NodeId,
        path: &'static str,
    ) -> Result<Production, ResolveError> {
        self.session
            .resolve_static_consumed(self.host, node, path)
            .map_err(|e: SessionResolveError| ResolveError::new(alloc::format!("{e}")))
    }

    fn resolve_static_bus(
        &mut self,
        scope: Option<ScopeRef>,
        channel: &'static str,
    ) -> Result<Production, ResolveError> {
        self.session
            .resolve_static_bus(self.host, scope, channel)
            .map_err(|e: SessionResolveError| ResolveError::new(alloc::format!("{e}")))
    }

    fn intern_key(&mut self, query: &QueryKey) -> alloc::rc::Rc<QueryKey> {
        self.session.intern_key(query)
    }

    fn produced_slot_path(&mut self, slot: &SlotPath) -> alloc::rc::Rc<SlotPath> {
        self.session.produced_slot_path(slot)
    }

    fn structure_epoch(&self) -> u64 {
        self.session.structure_epoch()
    }

    fn publish_produced_slot(
        &mut self,
        node: NodeId,
        slot: SlotPath,
        production: Production,
    ) -> Result<(), ResolveError> {
        self.session.publish_produced_slot(node, slot, production);
        Ok(())
    }

    fn node_scope(&self, node: NodeId) -> Option<ScopeRef> {
        self.host.node_scope(node)
    }

    fn consumed_slot_bus_provenance(
        &self,
        node: NodeId,
        slot: &SlotPath,
    ) -> Option<(ScopeRef, ChannelName)> {
        self.host.consumed_slot_bus_provenance(node, slot)
    }

    fn render_control(
        &mut self,
        product: ControlProduct,
        request: &ControlRenderRequest,
        target: ControlRenderTarget<'_>,
    ) -> Result<ControlLayout, ResolveError> {
        self.host
            .render_control(product, request, target)
            .map_err(|e: SessionResolveError| ResolveError::new(alloc::format!("{e}")))
    }

    fn control_patch_placement(
        &self,
        product: ControlProduct,
        consumer: NodeId,
    ) -> Option<Vec<PatchedRun>> {
        self.host.control_patch_placement(product, consumer)
    }

    fn register_output_identity(
        &mut self,
        node: NodeId,
        name: Option<alloc::string::String>,
        revision: Revision,
    ) -> Option<alloc::string::String> {
        self.host.register_output_identity(node, name, revision)
    }

    fn known_output_names(&self) -> (Vec<alloc::string::String>, Revision) {
        self.host.known_output_names()
    }

    fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, ResolveError> {
        self.host
            .render_texture(product, request)
            .map_err(|e: SessionResolveError| ResolveError::new(alloc::format!("{e}")))
    }

    fn runtime_buffer_mut(
        &mut self,
        id: RuntimeBufferId,
        frame: Revision,
    ) -> Result<&mut RuntimeBuffer, ResolveError> {
        self.host
            .runtime_buffer_mut(id, frame)
            .map_err(|e: SessionResolveError| ResolveError::new(alloc::format!("{e}")))
    }

    fn publish_timebase(
        &mut self,
        clock: NodeId,
        effective_seconds: f32,
        delta_seconds: f32,
        at: Revision,
    ) {
        self.host
            .publish_timebase(clock, effective_seconds, delta_seconds, at);
    }

    fn time_product_seconds(&self, product: TimeProduct) -> Result<f32, ResolveError> {
        self.host
            .time_product_seconds(product)
            .map_err(|e: SessionResolveError| ResolveError::new(alloc::format!("{e}")))
    }

    fn time_product_delta(&self, product: TimeProduct) -> Result<f32, ResolveError> {
        self.host
            .time_product_delta(product)
            .map_err(|e: SessionResolveError| ResolveError::new(alloc::format!("{e}")))
    }

    fn time_product_phasor(
        &mut self,
        product: TimeProduct,
        key: &PhasorKey,
        config: &PhasorConfig,
        reader: (NodeId, &SlotPath),
    ) -> Result<(f32, u32), ResolveError> {
        self.host
            .time_product_phasor(product, key, config, reader)
            .map_err(|e: SessionResolveError| ResolveError::new(alloc::format!("{e}")))
    }
}
