//! Optional runtime capability for nodes that can materialize visual products.

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
}
