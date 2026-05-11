//! Node-facing demand resolution facade ([`TickResolver`]) backed by session + host.

use crate::dataflow::resolver::production::Production;
use crate::dataflow::resolver::query_key::QueryKey;
use crate::dataflow::resolver::resolve_error::{ResolveError, SessionResolveError};
use crate::dataflow::resolver::resolve_host::ResolveHost;
use crate::dataflow::resolver::resolve_session::ResolveSession;
use crate::products::control::{
    ControlLayout, ControlProduct, ControlRenderRequest, ControlRenderTarget,
};
use crate::products::visual::{RenderTextureRequest, TextureRenderProduct, VisualProduct};
use crate::resource::{RuntimeBuffer, RuntimeBufferId};
use lpc_model::Revision;

/// Narrow API for [`crate::node::TickContext`] demand reads (`QueryKey` → [`Production`]).
pub trait TickResolver {
    fn resolve(&mut self, query: QueryKey) -> Result<Production, ResolveError>;

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

    fn runtime_buffer_mut(
        &mut self,
        id: RuntimeBufferId,
        frame: Revision,
    ) -> Result<&mut RuntimeBuffer, ResolveError>;
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
    fn resolve(&mut self, query: QueryKey) -> Result<Production, ResolveError> {
        self.session
            .resolve(self.host, query)
            .map_err(|e: SessionResolveError| ResolveError::new(alloc::format!("{e}")))
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
}
