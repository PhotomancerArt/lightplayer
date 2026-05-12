//! Optional runtime capability for nodes that can materialize visual products.

use lps_shared::TextureBuffer;

use crate::products::visual::{RenderTextureRequest, TextureRenderProduct, VisualProduct};

use super::{NodeError, RenderContext};

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
    /// copies it into `target`. Shader nodes should override this so fixture and
    /// output callers can own and reuse render targets without transient
    /// allocation.
    fn render_texture_into(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        target: &mut lp_shader::LpsTextureBuf,
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
        if bytes.len() != target.data().len() {
            return Err(NodeError::msg("render texture target byte length mismatch"));
        }
        target.data_mut().copy_from_slice(bytes);
        Ok(())
    }
}
