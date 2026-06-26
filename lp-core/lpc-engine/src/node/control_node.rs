//! Optional runtime capability for nodes that can materialize control products.

use lpc_model::ControlDisplayLayout;

use crate::products::control::{
    ControlLayout, ControlProduct, ControlRenderRequest, ControlRenderTarget,
};

use super::{ControlRenderContext, NodeError};

/// Node capability for rendering graph-level [`ControlProduct`] values.
pub trait ControlNode {
    fn render_control(
        &mut self,
        product: ControlProduct,
        request: &ControlRenderRequest,
        target: ControlRenderTarget<'_>,
        ctx: &mut ControlRenderContext<'_>,
    ) -> Result<ControlLayout, NodeError>;

    fn control_display_layout(
        &mut self,
        product: ControlProduct,
        ctx: &mut ControlRenderContext<'_>,
    ) -> Result<Option<ControlDisplayLayout>, NodeError> {
        let _ = (product, ctx);
        Ok(None)
    }
}
