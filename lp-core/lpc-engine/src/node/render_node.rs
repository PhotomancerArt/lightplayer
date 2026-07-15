//! Optional runtime capability for nodes that can materialize visual products.

use lp_gfx::TextureHandle;

use crate::products::visual::{
    RenderTextureRequest, TextureRenderProduct, VisualProduct, VisualSampleBufferRequest,
    VisualSampleTarget,
};

use super::{NodeError, RenderContext, err_ctx};

/// Node capability for materializing graph-level [`VisualProduct`] values.
pub trait RenderNode {
    fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        ctx: &mut RenderContext<'_>,
    ) -> Result<TextureRenderProduct, NodeError>;

    /// Render a visual product into a caller-owned texture target.
    ///
    /// The default implementation materializes an owned texture product and
    /// uploads it into `target`. Shader nodes should override this so fixture
    /// and output callers can own and reuse render targets without transient
    /// allocation.
    fn render_texture_into(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        target: &mut TextureHandle,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        let texture = self.render_texture(product, request, ctx)?;
        if texture.storage_format() != target.format()
            || texture.width() != target.width()
            || texture.height() != target.height()
        {
            return Err(NodeError::msg("render texture target shape mismatch"));
        }
        let bytes = texture
            .try_raw_bytes()
            .ok_or_else(|| NodeError::msg("render texture product has no raw bytes"))?;
        ctx.graphics()
            .ok_or_else(|| NodeError::msg("render context has no graphics backend"))?
            .write_texture(target, bytes)
            .map_err(err_ctx("render texture target write"))
    }

    /// Sample a visual product at caller-provided points into caller-owned RGBA16 storage.
    fn sample_visual_into(
        &mut self,
        _product: VisualProduct,
        _request: VisualSampleBufferRequest<'_>,
        _target: VisualSampleTarget<'_>,
        _ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        Err(NodeError::msg(
            "render node does not support direct sampling",
        ))
    }
}
