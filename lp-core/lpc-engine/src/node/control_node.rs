//! Optional runtime capability for nodes that can materialize control products.

use crate::control_product::{
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
}
