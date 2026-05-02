//! Node-facing demand resolution facade ([`TickResolver`]) backed by session + host.

use crate::render_product::{RenderProductId, RenderSampleBatch, RenderSampleBatchResult};
use crate::resolver::production::Production;
use crate::resolver::query_key::QueryKey;
use crate::resolver::resolve_error::{ResolveError, SessionResolveError};
use crate::resolver::resolve_host::ResolveHost;
use crate::resolver::resolve_session::ResolveSession;
use crate::runtime_buffer::{RuntimeBuffer, RuntimeBufferId};
use lpc_model::FrameId;

/// Narrow API for [`crate::node::TickContext`] demand reads (`QueryKey` → [`Production`]).
pub trait TickResolver {
    fn resolve(&mut self, query: QueryKey) -> Result<Production, ResolveError>;

    fn sample_render_product(
        &mut self,
        id: RenderProductId,
        batch: &RenderSampleBatch,
    ) -> Result<RenderSampleBatchResult, ResolveError>;

    fn runtime_buffer_mut(
        &mut self,
        id: RuntimeBufferId,
        frame: FrameId,
    ) -> Result<&mut RuntimeBuffer, ResolveError>;
}

/// Bridges [`ResolveSession`] + [`ResolveHost`] into a [`TickResolver`].
///
/// `'resolver` is the session's resolver/registry borrow ([`ResolveSession`]'s lifetime parameter).
/// `'sess` is the borrow of that session from the caller (often shorter); splitting them avoids
/// invariant `'sess == 'resolver` churn when constructing from `&mut ResolveSession<'resolver>`.
pub struct SessionHostResolver<'sess, 'resolver, 'host> {
    pub session: &'sess mut ResolveSession<'resolver>,
    pub host: &'host mut dyn ResolveHost,
}

impl<'sess, 'resolver, 'host> TickResolver for SessionHostResolver<'sess, 'resolver, 'host> {
    fn resolve(&mut self, query: QueryKey) -> Result<Production, ResolveError> {
        self.session
            .resolve(self.host, query)
            .map_err(|e: SessionResolveError| ResolveError::new(alloc::format!("{e}")))
    }

    fn sample_render_product(
        &mut self,
        id: RenderProductId,
        batch: &RenderSampleBatch,
    ) -> Result<RenderSampleBatchResult, ResolveError> {
        self.host
            .sample_render_product(id, batch)
            .map_err(|e: SessionResolveError| ResolveError::new(alloc::format!("{e}")))
    }

    fn runtime_buffer_mut(
        &mut self,
        id: RuntimeBufferId,
        frame: FrameId,
    ) -> Result<&mut RuntimeBuffer, ResolveError> {
        self.host
            .runtime_buffer_mut(id, frame)
            .map_err(|e: SessionResolveError| ResolveError::new(alloc::format!("{e}")))
    }
}
