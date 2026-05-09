//! Optional runtime capability for nodes that can materialize render products.

use crate::render_product::{RenderProduct, RenderTextureRequest, TextureRenderProduct};

use super::{NodeError, RenderContext};

/// Node capability for materializing graph-level [`RenderProduct`] values.
pub trait RenderNode {
    fn render_texture(
        &mut self,
        product: RenderProduct,
        request: &RenderTextureRequest,
        ctx: &mut RenderContext<'_>,
    ) -> Result<TextureRenderProduct, NodeError>;
}
