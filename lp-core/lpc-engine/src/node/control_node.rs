//! Optional runtime capability for nodes that can materialize control products.

use lpc_model::{ControlDisplayLayout, Revision};

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

    /// The revision [`Self::control_display_layout`] would stamp on its
    /// layout, without building the layout.
    ///
    /// The published-frame read asks every producer for this each time and
    /// builds the layout only when the client does not already hold that
    /// revision. A node that can answer from its own cached state overrides
    /// this; the default builds the layout and reads the revision off it, so
    /// the two always agree. `Ok(None)` exactly when
    /// [`Self::control_display_layout`] is.
    fn control_display_layout_revision(
        &mut self,
        product: ControlProduct,
        ctx: &mut ControlRenderContext<'_>,
    ) -> Result<Option<Revision>, NodeError> {
        Ok(self
            .control_display_layout(product, ctx)?
            .map(|layout| layout.revision()))
    }
}
