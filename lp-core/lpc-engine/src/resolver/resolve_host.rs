//! [`ResolveHost`] — callback for uncached [`crate::resolver::QueryKey::ProducedSlot`] (and
//! unbound [`crate::resolver::QueryKey::ConsumedSlot`]) production.

use crate::render_product::{
    NativeTexturePayload, RenderProductError, RenderProductId, RenderSampleBatch,
    RenderSampleBatchResult,
};
use crate::resolver::production::Production;
use crate::resolver::query_key::QueryKey;
use crate::resolver::resolve_error::SessionResolveError;
use crate::resolver::resolve_session::ResolveSession;
use crate::runtime_buffer::{RuntimeBuffer, RuntimeBufferId};
use lpc_model::FrameId;

/// Engine or test fake that can satisfy demand for uncached queries.
pub trait ResolveHost {
    fn produce(
        &mut self,
        query: &QueryKey,
        session: &mut ResolveSession<'_>,
    ) -> Result<Production, SessionResolveError>;

    fn sample_render_product(
        &mut self,
        id: RenderProductId,
        batch: &RenderSampleBatch,
    ) -> Result<RenderSampleBatchResult, SessionResolveError> {
        let _ = (id, batch);
        Err(SessionResolveError::other(
            "resolve host has no render product sampler",
        ))
    }

    fn with_native_texture_payload(
        &mut self,
        id: RenderProductId,
        visitor: &mut dyn FnMut(NativeTexturePayload<'_>),
    ) -> Result<(), SessionResolveError> {
        let _ = (id, visitor);
        Err(SessionResolveError::other(
            "resolve host has no native texture payload access",
        ))
    }

    fn runtime_buffer_mut(
        &mut self,
        id: RuntimeBufferId,
        frame: FrameId,
    ) -> Result<&mut RuntimeBuffer, SessionResolveError> {
        let _ = (id, frame);
        Err(SessionResolveError::other(
            "resolve host has no runtime buffer writer",
        ))
    }
}

impl From<RenderProductError> for SessionResolveError {
    fn from(value: RenderProductError) -> Self {
        SessionResolveError::other(alloc::format!("render product: {value:?}"))
    }
}
