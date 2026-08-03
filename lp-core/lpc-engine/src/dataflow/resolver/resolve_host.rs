//! [`ResolveHost`] — callback for uncached [`crate::dataflow::resolver::QueryKey::ProducedSlot`] (and
//! unbound [`crate::dataflow::resolver::QueryKey::ConsumedSlot`]) production.

use crate::dataflow::resolver::production::Production;
use crate::dataflow::resolver::query_key::QueryKey;
use crate::dataflow::resolver::resolve_error::SessionResolveError;
use crate::dataflow::resolver::resolve_session::ResolveSession;
use crate::products::control::{
    ControlLayout, ControlProduct, ControlRenderRequest, ControlRenderTarget,
};
use crate::products::visual::{RenderTextureRequest, TextureRenderProduct, VisualProduct};
use crate::resource::{RuntimeBuffer, RuntimeBufferId};
use alloc::vec::Vec;
use lpc_model::{ChannelName, NodeId, Revision, SlotMerge, SlotPath};

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
}
