//! The project node's runtime: mirror the scope's `visual.out` as produced
//! `output` (scoped-buses ADR, rule 5).

use alloc::format;
use alloc::string::String;

use lp_gfx::TextureHandle;
use lpc_model::{ChannelName, FromLpValue, NodeId, SlotPath, VisualProduct, VisualProductSlot};
use lps_shared::TextureStorageFormat;

use crate::dataflow::bus::{ScopeId, ScopedChannel};
use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, ProduceResult,
    RenderContext, RenderNode, RuntimeStateShape, TickContext, err_ctx,
};
use crate::products::visual::{
    RenderTextureRequest, TextureRenderProduct, VisualSampleBufferRequest, VisualSampleTarget,
};
use lpc_model::{SlotAccess, SlotShapeRegistry, SlotShapeRegistryError};

use super::ProjectMirrorState;

/// Runtime node attached to every project node, root included (uniformity
/// is the point — the root's mirror is simply unread today).
///
/// The playlist pattern, minus the blending: `produce` resolves the
/// project's own scope's `visual.out` channel and remembers the producing
/// node's [`VisualProduct`] handle, while the published `output` row always
/// carries the project node's *own* handle — render dispatch then forwards
/// to the remembered producer. A scope with no visual writer forwards
/// nothing and renders cleared (a project without a visual is a legitimate
/// shape, not an error). No bindings are registered for the mirror — it has
/// zero binding-graph footprint.
pub struct ProjectNode {
    /// The scope this project node introduces, pinned to `visual.out`.
    channel: ScopedChannel,
    /// The scope's current visual producer, refreshed each `produce`.
    mirrored: Option<VisualProduct>,
    state: ProjectMirrorState,
}

impl ProjectNode {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            channel: ScopedChannel::new(
                ScopeId::Project(node_id),
                ChannelName(String::from(lpc_model::PRIMARY_VISUAL_CHANNEL)),
            ),
            mirrored: None,
            state: ProjectMirrorState {
                output: VisualProductSlot::new(VisualProduct::new(node_id, 0)),
            },
        }
    }

    fn output_path() -> SlotPath {
        SlotPath::parse("output").expect("project output path")
    }
}

impl NodeRuntime for ProjectNode {
    fn produce(
        &mut self,
        _slot: &SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        // `None` = the scope has no `visual.out` writer: nothing to
        // forward; renders clear below.
        self.mirrored = match ctx
            .resolve_bus(self.channel.clone())
            .map_err(|e| NodeError::msg(format!("resolve scope visual channel: {}", e.message)))?
        {
            Some(production) => {
                let value = production
                    .value_leaf()
                    .ok_or_else(|| NodeError::msg("scope visual channel is not a value"))?;
                Some(
                    VisualProduct::from_lp_value(value.value())
                        .map_err(|e| NodeError::msg(format!("scope visual channel: {e}")))?,
                )
            }
            None => None,
        };
        self.state
            .output
            .set_with_version(ctx.revision(), *self.state.output.value());
        ctx.publish_runtime_slot(&self.state, Self::output_path())?;
        Ok(ProduceResult::Produced)
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError> {
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: PressureLevel,
        _ctx: &mut MemPressureCtx<'_>,
    ) -> Result<(), NodeError> {
        Ok(())
    }

    fn runtime_state_slots(&self) -> Option<&dyn SlotAccess> {
        Some(&self.state)
    }

    fn register_runtime_state_shapes(
        &self,
        registry: &mut SlotShapeRegistry,
    ) -> Result<(), SlotShapeRegistryError> {
        ProjectMirrorState::register_runtime_state_shape(registry).map(|_| ())
    }

    fn render_node(&mut self) -> Option<&mut dyn RenderNode> {
        Some(self)
    }
}

impl RenderNode for ProjectNode {
    fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        ctx: &mut RenderContext<'_>,
    ) -> Result<TextureRenderProduct, NodeError> {
        if request.format != TextureStorageFormat::Rgba16Unorm {
            return Err(NodeError::msg(
                "project mirror texture render only supports RGBA16 unorm",
            ));
        }
        let mut texture = {
            let graphics = ctx
                .graphics()
                .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
            graphics
                .create_render_target(request.width, request.height)
                .map_err(err_ctx("project mirror texture"))?
        };
        self.render_texture_into(product, request, &mut texture, ctx)?;
        let graphics = ctx.graphics().expect("graphics checked above");
        if !graphics.supports_read_back() {
            return TextureRenderProduct::gpu_resident(texture)
                .map_err(err_ctx("project mirror gpu texture product"));
        }
        let bytes = graphics
            .read_back(&texture)
            .map_err(err_ctx("project mirror read back"))?
            .into_bytes();
        TextureRenderProduct::rgba16_unorm(request.width, request.height, bytes)
            .map_err(err_ctx("project mirror texture product"))
    }

    fn render_texture_into(
        &mut self,
        _product: VisualProduct,
        request: &RenderTextureRequest,
        target: &mut TextureHandle,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        let Some(mirrored) = self.mirrored else {
            return ctx
                .graphics()
                .ok_or_else(|| NodeError::msg("missing graphics backend"))?
                .clear_texture(target)
                .map_err(err_ctx("project mirror clear target"));
        };
        ctx.render_texture_into(mirrored, request, target)
    }

    fn sample_visual_into(
        &mut self,
        _product: VisualProduct,
        request: VisualSampleBufferRequest<'_>,
        target: VisualSampleTarget<'_>,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        let Some(mirrored) = self.mirrored else {
            return ctx
                .graphics()
                .ok_or_else(|| NodeError::msg("missing graphics backend"))?
                .clear_sample_out(target.samples)
                .map_err(err_ctx("project mirror clear samples"));
        };
        ctx.sample_visual_into(mirrored, request, target)
    }
}
