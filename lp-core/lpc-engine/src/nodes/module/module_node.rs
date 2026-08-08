//! The module node's runtime: mirror the introduced scope's `visual.out`
//! as produced `output` (modules.md R7).
//!
//! Attached to every module node, root included — uniformity is the point
//! (the root is an ordinary module that happens to be at the root; its
//! mirror is what "play the project" renders). Harvested from the
//! superseded composite-effects spike's `ProjectNode` with the scope
//! arriving as a [`QueryKey::Bus`] key instead of the spike's side-table
//! `ScopedChannel`.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;

use lp_gfx::TextureHandle;
use lpc_model::{ChannelName, FromLpValue, NodeId, SlotPath, VisualProduct, VisualProductSlot};
use lps_shared::TextureStorageFormat;

use crate::dataflow::resolver::QueryKey;
use crate::node::RuntimeStateShape;
use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, ProduceResult,
    RenderContext, RenderNode, ScopeRef, TickContext, err_ctx,
};
use crate::products::visual::{
    RenderTextureRequest, TextureRenderProduct, VisualSampleBufferRequest, VisualSampleTarget,
};
use lpc_model::{SlotAccess, SlotShapeRegistry, SlotShapeRegistryError};

use super::ModuleMirrorState;

/// Runtime node attached to every module node.
///
/// The playlist pattern, minus the blending: `produce` resolves the
/// module's own introduced scope's `visual.out` channel and remembers the
/// producing node's [`VisualProduct`] handle, while the published `output`
/// row always carries the module node's *own* handle — render dispatch
/// then forwards to the remembered producer. A scope with no visual
/// writer forwards nothing and renders cleared.
pub struct ModuleNode {
    /// The scoped demand key for this module's own `visual.out`.
    channel: QueryKey,
    /// The scope's current visual producer, refreshed each `produce`.
    mirrored: Option<VisualProduct>,
    state: ModuleMirrorState,
}

impl ModuleNode {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            channel: QueryKey::Bus {
                scope: Some(ScopeRef::Module { owner: node_id }),
                channel: ChannelName(String::from(lpc_model::PRIMARY_VISUAL_CHANNEL)),
            },
            mirrored: None,
            state: ModuleMirrorState {
                output: VisualProductSlot::new(VisualProduct::new(node_id, 0)),
            },
        }
    }

    pub fn boxed(node_id: NodeId) -> Box<dyn NodeRuntime> {
        Box::new(Self::new(node_id))
    }

    fn output_path() -> SlotPath {
        SlotPath::parse("output").expect("module output path")
    }
}

impl NodeRuntime for ModuleNode {
    fn produce(
        &mut self,
        _slot: &SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        // No writer anywhere for the scope's visual.out = a legitimate
        // module-without-a-visual: mirror cleared (R7), never an error.
        self.mirrored = match ctx.resolve(&self.channel) {
            Ok(production) => {
                let value = production
                    .value_leaf()
                    .ok_or_else(|| NodeError::msg("scope visual channel is not a value"))?;
                Some(
                    VisualProduct::from_lp_value(value.value())
                        .map_err(|e| NodeError::msg(format!("scope visual channel: {e}")))?,
                )
            }
            Err(error) if error.is_no_bus_provider() => None,
            Err(error) => {
                return Err(NodeError::msg(format!(
                    "resolve scope visual channel: {}",
                    error.message
                )));
            }
        };
        self.state
            .output
            .set_with_version(ctx.revision(), *self.state.output.value());
        ctx.publish_runtime_slot(&self.state, Self::output_path())?;
        Ok(ProduceResult::Produced)
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx) -> Result<(), NodeError> {
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: PressureLevel,
        _ctx: &mut MemPressureCtx,
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
        ModuleMirrorState::register_runtime_state_shape(registry).map(|_| ())
    }

    fn render_node(&mut self) -> Option<&mut dyn RenderNode> {
        Some(self)
    }
}

impl RenderNode for ModuleNode {
    /// A mirror has no space of its own: it answers with the mirrored
    /// product's, or 2D when it mirrors nothing.
    fn visual_space(
        &mut self,
        _product: VisualProduct,
        ctx: &mut RenderContext<'_>,
    ) -> Result<crate::products::visual::ProductSpaceInfo, NodeError> {
        let Some(mirrored) = self.mirrored else {
            return Ok(crate::products::visual::ProductSpaceInfo::two_d());
        };
        ctx.visual_product_space(mirrored)
    }

    fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        ctx: &mut RenderContext<'_>,
    ) -> Result<TextureRenderProduct, NodeError> {
        if request.format != TextureStorageFormat::Rgba16Unorm {
            return Err(NodeError::msg(
                "module mirror texture render only supports RGBA16 unorm",
            ));
        }
        let mut texture = {
            let graphics = ctx
                .graphics()
                .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
            graphics
                .create_render_target(request.width, request.height)
                .map_err(err_ctx("module mirror texture"))?
        };
        self.render_texture_into(product, request, &mut texture, ctx)?;
        let graphics = ctx.graphics().expect("graphics checked above");
        if !graphics.supports_read_back() {
            return TextureRenderProduct::gpu_resident(texture)
                .map_err(err_ctx("module mirror gpu texture product"));
        }
        let bytes = graphics
            .read_back(&texture)
            .map_err(err_ctx("module mirror read back"))?
            .into_bytes();
        TextureRenderProduct::rgba16_unorm(request.width, request.height, bytes)
            .map_err(err_ctx("module mirror texture product"))
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
                .map_err(err_ctx("module mirror clear target"));
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
                .map_err(err_ctx("module mirror clear samples"));
        };
        ctx.sample_visual_into(mirrored, request, target)
    }
}
