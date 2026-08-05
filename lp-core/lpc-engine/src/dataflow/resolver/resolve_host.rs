//! [`ResolveHost`] — callback for uncached [`crate::dataflow::resolver::QueryKey::ProducedSlot`] (and
//! unbound [`crate::dataflow::resolver::QueryKey::ConsumedSlot`]) production.

use crate::dataflow::resolver::production::Production;
use crate::dataflow::resolver::query_key::QueryKey;
use crate::dataflow::resolver::resolve_error::SessionResolveError;
use crate::dataflow::resolver::resolve_session::ResolveSession;
use crate::dataflow::timebase::PhasorKey;
use crate::products::control::{
    ControlLayout, ControlProduct, ControlRenderRequest, ControlRenderTarget,
};
use crate::products::visual::{RenderTextureRequest, TextureRenderProduct, VisualProduct};
use crate::resource::{RuntimeBuffer, RuntimeBufferId};
use alloc::vec::Vec;
use lpc_model::{ChannelName, NodeId, PhasorConfig, Revision, SlotMerge, SlotPath, TimeProduct};

use crate::dataflow::binding::{BindingEntry, BindingRef};
use crate::node::ScopeRef;

/// Engine or test fake that can satisfy demand for uncached queries.
pub trait ResolveHost {
    fn produce(
        &mut self,
        query: &QueryKey,
        session: &mut ResolveSession<'_>,
    ) -> Result<Production, SessionResolveError>;

    fn binding_for_consumed_slot(
        &self,
        _node: NodeId,
        _slot: &SlotPath,
    ) -> Option<(BindingRef, BindingEntry)> {
        None
    }

    fn bindings_for_consumed_slot(
        &self,
        node: NodeId,
        slot: &SlotPath,
    ) -> Vec<(BindingRef, BindingEntry)> {
        self.binding_for_consumed_slot(node, slot)
            .into_iter()
            .collect()
    }

    fn merge_policy_for_consumed_slot(&self, _node: NodeId, _slot: &SlotPath) -> SlotMerge {
        SlotMerge::Latest
    }

    /// Where a consumed slot's value comes from, when it comes from a bus
    /// channel with a live writer: the channel plus the scope that writer
    /// lives in, after the R5 shadowing walk outward from the reader.
    ///
    /// This is the *provenance* a phasor identity is derived from (parent
    /// D3), which is why it answers `None` in the two cases that mean
    /// "slot-local": the slot is not bus-bound at all, and the channel it
    /// binds has no writer anywhere (an R6 fallback reads the authored
    /// default, so it is exactly as private as an unbound slot).
    fn consumed_slot_bus_provenance(
        &self,
        _node: NodeId,
        _slot: &SlotPath,
    ) -> Option<(ScopeRef, ChannelName)> {
        None
    }

    /// The bus scope `node` writes into and reads from (its inhabited
    /// scope; the root module reads its own introduced scope). `None`
    /// means the host has no scope model — test fakes — and every read
    /// shares the unscoped key.
    fn node_scope(&self, _node: NodeId) -> Option<ScopeRef> {
        None
    }

    /// The winning provider set for a bus read performed from `scope`:
    /// writer-shadowing (modules.md R5) resolves outward to the nearest
    /// enclosing scope with at least one provider — entirely host-side, so
    /// the resolver itself stays scope-dumb. `scope: None` (scopeless
    /// hosts) answers with the flat provider set.
    fn providers_for_bus(
        &self,
        _scope: Option<ScopeRef>,
        _channel: &ChannelName,
    ) -> Vec<(BindingRef, BindingEntry)> {
        Vec::new()
    }

    fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, SessionResolveError> {
        let _ = (product, request);
        Err(SessionResolveError::other(
            "resolve host has no render texture access",
        ))
    }

    fn render_control(
        &mut self,
        product: ControlProduct,
        request: &ControlRenderRequest,
        target: ControlRenderTarget<'_>,
    ) -> Result<ControlLayout, SessionResolveError> {
        let _ = (product, request, target);
        Err(SessionResolveError::other(
            "resolve host has no render control access",
        ))
    }

    fn runtime_buffer_mut(
        &mut self,
        id: RuntimeBufferId,
        frame: Revision,
    ) -> Result<&mut RuntimeBuffer, SessionResolveError> {
        let _ = (id, frame);
        Err(SessionResolveError::other(
            "resolve host has no runtime buffer writer",
        ))
    }

    /// Publish `clock`'s timebase for this tick into the host's timebase
    /// store. Called by a timebase-producing node from its `produce`.
    ///
    /// Defaulted to a no-op: a host without a store simply cannot answer
    /// [`Self::time_product_seconds`] and friends, and a producer should not
    /// have to care which kind of host it is running under.
    fn publish_timebase(
        &mut self,
        clock: NodeId,
        effective_seconds: f32,
        delta_seconds: f32,
        at: Revision,
    ) {
        let _ = (clock, effective_seconds, delta_seconds, at);
    }

    fn time_product_seconds(&self, product: TimeProduct) -> Result<f32, SessionResolveError> {
        let _ = product;
        Err(SessionResolveError::other(
            "resolve host has no timebase store",
        ))
    }

    fn time_product_delta(&self, product: TimeProduct) -> Result<f32, SessionResolveError> {
        let _ = product;
        Err(SessionResolveError::other(
            "resolve host has no timebase store",
        ))
    }

    /// Tick-side phasor read: materializes on first ask and advances once per
    /// tick. Returns the raw wrapped ramp — waveform and phase offset are the
    /// caller's to apply.
    ///
    /// `reader` is the consuming node and the consumed slot the config was
    /// resolved for; the store records it (with the config's shaping) as
    /// witness data for the timebase probe. It never affects the answer.
    fn time_product_phasor(
        &mut self,
        product: TimeProduct,
        key: &PhasorKey,
        config: &PhasorConfig,
        reader: (NodeId, &SlotPath),
    ) -> Result<(f32, u32), SessionResolveError> {
        let _ = (product, key, config, reader);
        Err(SessionResolveError::other(
            "resolve host has no timebase store",
        ))
    }
}
