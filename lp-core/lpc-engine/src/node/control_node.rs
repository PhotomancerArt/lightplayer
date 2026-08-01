//! Optional runtime capability for nodes that can materialize control products.

use lp_gfx::TextureHandle;
use lpc_model::ControlDisplayLayout;

use crate::products::control::{
    ControlLayout, ControlPreviewSpec, ControlProduct, ControlRenderRequest, ControlRenderTarget,
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

    /// Describe the GPU-resident fixture preview for `product`: display
    /// layout, sample-point count, and the Display-policy color factor.
    /// `Ok(None)` means the node has no resident preview path (the engine
    /// reports that explicitly — no silent fallback).
    fn control_preview_spec(
        &mut self,
        product: ControlProduct,
        ctx: &mut ControlRenderContext<'_>,
    ) -> Result<Option<ControlPreviewSpec>, NodeError> {
        let _ = (product, ctx);
        Ok(None)
    }

    /// Evaluate the product's sample points GPU-resident into the
    /// caller-owned `grid` render target for the current frame: texel `i`
    /// receives the raw sampled color of
    /// [`ControlPreviewSpec::layout`]`.lamps[i]` (no brightness, gamma, or
    /// color order — color policy is applied at draw time via
    /// [`ControlPreviewSpec::color_scale`]).
    fn render_control_preview_grid(
        &mut self,
        product: ControlProduct,
        grid: &mut TextureHandle,
        ctx: &mut ControlRenderContext<'_>,
    ) -> Result<(), NodeError> {
        let _ = (product, grid, ctx);
        Err(NodeError::msg(
            "control node does not support resident preview grids",
        ))
    }
}
