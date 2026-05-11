//! [`ResolveHost`] — callback for uncached [`crate::resolver::QueryKey::ProducedSlot`] (and
//! unbound [`crate::resolver::QueryKey::ConsumedSlot`]) production.

use crate::control_product::{
    ControlLayout, ControlProduct, ControlRenderRequest, ControlRenderTarget,
};
use crate::resolver::production::Production;
use crate::resolver::query_key::QueryKey;
use crate::resolver::resolve_error::SessionResolveError;
use crate::resolver::resolve_session::ResolveSession;
use crate::runtime_buffer::{RuntimeBuffer, RuntimeBufferId};
use crate::visual_product::{RenderTextureRequest, TextureRenderProduct, VisualProduct};
use alloc::vec::Vec;
use lpc_model::{ChannelName, NodeId, Revision, SlotPath};

use crate::binding::{BindingEntry, BindingRef};

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

    fn providers_for_bus(&self, _channel: &ChannelName) -> Vec<(BindingRef, BindingEntry)> {
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
