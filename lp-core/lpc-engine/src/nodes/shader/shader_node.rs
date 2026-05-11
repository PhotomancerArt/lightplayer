//! Core shader node: owns GLSL compilation/rendering and exposes output as [`RuntimeProduct::Render`].

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;

use lpc_model::nodes::shader::ShaderDef;
use lpc_model::{
    AddSubMode, DivMode, GlslOpts, MulMode, NodeId, ShaderState, SlotAccess, SlotPath,
    SlotShapeRegistry, SlotShapeRegistryError, StaticSlotShape,
};
use lps_shared::TextureBuffer;

use crate::gfx::{LpShader, ShaderCompileOptions};
use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, RenderContext, RenderNode,
    TickContext,
};
use crate::render_product::{RenderProduct, RenderTextureRequest, TextureRenderProduct};
/// Default max semantic errors forwarded from the GLSL to LPIR front end.
const SHADER_COMPILE_MAX_ERRORS: usize = 20;

/// Shader producer wired to the core engine.
pub struct ShaderNode {
    node_id: NodeId,
    config: ShaderDef,
    glsl_source: String,
    shader: Option<Box<dyn LpShader>>,
    compilation_error: Option<String>,
    state: ShaderState,
}

impl ShaderNode {
    pub fn new(node_id: NodeId, config: ShaderDef, glsl_source: String) -> Self {
        Self {
            node_id,
            config,
            glsl_source,
            shader: None,
            compilation_error: None,
            state: ShaderState::new(RenderProduct::new(node_id, 0)),
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn render_product(&self) -> RenderProduct {
        *self.state.output.value()
    }

    pub fn compilation_error(&self) -> Option<&str> {
        self.compilation_error.as_deref()
    }

    fn ensure_compiled(&mut self, ctx: &RenderContext<'_>) -> Result<(), NodeError> {
        if self.shader.is_some() {
            return Ok(());
        }

        let graphics = ctx
            .graphics()
            .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
        log::info!(
            "[shader-node] compilation starting (node={:?}, {} bytes)",
            self.node_id,
            self.glsl_source.len()
        );
        lp_perf::emit_begin!(lp_perf::EVENT_SHADER_COMPILE);
        self.compilation_error = None;
        let compile_opts = ShaderCompileOptions {
            q32_options: map_model_q32_options(&self.config.glsl_opts),
            max_errors: Some(SHADER_COMPILE_MAX_ERRORS),
        };

        #[cfg(feature = "panic-recovery")]
        let compile_result: Result<Box<dyn LpShader>, String> = {
            use core::panic::AssertUnwindSafe;
            use unwinding::panic::catch_unwind;
            match catch_unwind(AssertUnwindSafe(|| {
                graphics.compile_shader(self.glsl_source.as_str(), &compile_opts)
            })) {
                Ok(inner) => inner.map_err(|e| format!("{e}")),
                Err(_) => Err(String::from("OOM during shader compilation")),
            }
        };
        #[cfg(not(feature = "panic-recovery"))]
        let compile_result: Result<Box<dyn LpShader>, String> = graphics
            .compile_shader(self.glsl_source.as_str(), &compile_opts)
            .map_err(|e| format!("{e}"));
        lp_perf::emit_end!(lp_perf::EVENT_SHADER_COMPILE);

        match compile_result {
            Ok(shader) => {
                self.shader = Some(shader);
                log::info!(
                    "[shader-node] compilation succeeded (node={:?})",
                    self.node_id
                );
                Ok(())
            }
            Err(error) => {
                self.compilation_error = Some(error.clone());
                self.shader = None;
                log::warn!(
                    "[shader-node] compilation failed (node={:?}): {error}",
                    self.node_id
                );
                Err(NodeError::msg(format!("shader compile: {error}")))
            }
        }
    }
}

impl NodeRuntime for ShaderNode {
    fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        self.state
            .output
            .set_with_version(ctx.revision(), RenderProduct::new(self.node_id, 0));
        Ok(())
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

    fn runtime_state_slots(&self) -> &dyn SlotAccess {
        &self.state
    }

    fn register_runtime_state_shapes(
        &self,
        registry: &mut SlotShapeRegistry,
    ) -> Result<(), SlotShapeRegistryError> {
        ShaderState::ensure_registered(registry).map(|_| ())
    }

    fn render_node(&mut self) -> Option<&mut dyn RenderNode> {
        Some(self)
    }
}

pub fn shader_output_path() -> SlotPath {
    SlotPath::parse("output").expect("shader output path")
}

impl RenderNode for ShaderNode {
    fn render_texture(
        &mut self,
        product: RenderProduct,
        request: &RenderTextureRequest,
        ctx: &mut RenderContext<'_>,
    ) -> Result<TextureRenderProduct, NodeError> {
        if product.node() != self.node_id {
            return Err(NodeError::msg(format!(
                "shader node {:?} cannot render product owned by {:?}",
                self.node_id,
                product.node()
            )));
        }
        if product.output() != 0 {
            return Err(NodeError::msg(format!(
                "shader node {:?} has no render output {}",
                self.node_id,
                product.output()
            )));
        }

        self.ensure_compiled(ctx)?;
        let shader = self
            .shader
            .as_mut()
            .ok_or_else(|| NodeError::msg("shader missing after compile"))?;
        if !shader.has_render() {
            return Err(NodeError::msg("compiled shader has no render() entry"));
        }

        let graphics = ctx
            .graphics()
            .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
        let mut texture = graphics
            .alloc_output_buffer(request.width, request.height)
            .map_err(|e| NodeError::msg(format!("alloc_output_buffer: {e}")))?;
        if texture.format() != request.format {
            return Err(NodeError::msg(format!(
                "graphics allocated {:?}, requested {:?}",
                texture.format(),
                request.format
            )));
        }
        shader
            .render(&mut texture, request.time_seconds)
            .map_err(|e| NodeError::msg(format!("shader render: {e}")))?;

        TextureRenderProduct::new(
            texture.width(),
            texture.height(),
            texture.format(),
            texture.data().to_vec(),
        )
        .map_err(|e| NodeError::msg(format!("texture product: {e}")))
    }
}

fn map_model_q32_options(opts: &GlslOpts) -> lps_q32::q32_options::Q32Options {
    lps_q32::q32_options::Q32Options {
        add_sub: match opts.add_sub.value() {
            AddSubMode::Saturating => lps_q32::q32_options::AddSubMode::Saturating,
            AddSubMode::Wrapping => lps_q32::q32_options::AddSubMode::Wrapping,
        },
        mul: match opts.mul.value() {
            MulMode::Saturating => lps_q32::q32_options::MulMode::Saturating,
            MulMode::Wrapping => lps_q32::q32_options::MulMode::Wrapping,
        },
        div: match opts.div.value() {
            DivMode::Saturating => lps_q32::q32_options::DivMode::Saturating,
            DivMode::Reciprocal => lps_q32::q32_options::DivMode::Reciprocal,
        },
    }
}

#[cfg(test)]
mod tests {
    use alloc::sync::Arc;
    use alloc::vec;

    use super::*;
    use crate::engine::Engine;
    use crate::engine::resolve_with_engine_host;
    use crate::node::test_placeholder_spine;
    use crate::nodes::TextureNode;
    use crate::render_product::{RenderProduct, RenderSampleBatch, RenderSamplePoint};
    use crate::resolver::QueryKey;
    use crate::resolver::ResolveLogLevel;
    use lpc_model::{Revision, SlotDataAccess, StaticSlotShape, TreePath};
    use lpc_wire::{WireChildKind, WireSlotIndex};

    const DEMO_GLSL: &str = "layout(binding = 0) uniform vec2 outputSize; layout(binding = 1) uniform float time; vec4 render(vec2 pos) { return vec4(mod(time, 1.0), 0.0, 0.0, 1.0); }";

    fn build_texture_and_shader_engine() -> (Engine, NodeId, NodeId, RenderProduct) {
        let mut engine = Engine::new(TreePath::parse("/show.t").expect("path"));
        engine.set_graphics(Some(Arc::new(crate::Graphics::new())));
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let (spine, artifact) = test_placeholder_spine();

        let tex_id = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("tex").expect("name"),
                lpc_model::NodeName::parse("texture").expect("ty"),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                spine.clone(),
                artifact,
                frame,
            )
            .expect("texture");

        let tex = TextureNode::new(tex_id);
        engine
            .attach_runtime_node(tex_id, Box::new(tex), frame)
            .expect("attach tex");

        let sh_id = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("sh").expect("name"),
                lpc_model::NodeName::parse("shader").expect("ty"),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                spine,
                artifact,
                frame,
            )
            .expect("shader");

        let cfg = ShaderDef::default();

        let sh = ShaderNode::new(sh_id, cfg, String::from(DEMO_GLSL));
        engine
            .attach_runtime_node(sh_id, Box::new(sh), frame)
            .expect("attach shader");

        let rid = RenderProduct::new(sh_id, 0);

        (engine, tex_id, sh_id, rid)
    }

    #[test]
    fn shader_render_output_is_on_runtime_state_slot_root() {
        let cfg = ShaderDef::default();
        let node = ShaderNode::new(NodeId::new(1), cfg, String::new());

        assert_eq!(node.runtime_state_slots().shape_id(), ShaderState::SHAPE_ID);
        let SlotDataAccess::Record(record) = node.runtime_state_slots().data() else {
            panic!("shader runtime state should be a record");
        };
        let Some(SlotDataAccess::Value(output)) = record.field(0) else {
            panic!("shader runtime state output should be a value");
        };

        assert_eq!(
            output.value(),
            lpc_model::LpValue::RenderProduct(node.render_product())
        );
    }

    #[test]
    fn shader_core_produces_render_runtime_product() {
        let (mut engine, _tex_id, sh_id, rid) = build_texture_and_shader_engine();
        engine.tick(1000).expect("tick");

        let q = QueryKey::ProducedSlot {
            node: sh_id,
            slot: shader_output_path(),
        };
        let prod = resolve_with_engine_host(&mut engine, q, ResolveLogLevel::Off)
            .expect("resolve")
            .0;
        let rp = prod.product.get();
        let got_id = rp.as_render().expect("render product");
        assert_eq!(got_id, rid);
    }

    #[test]
    fn shader_core_render_product_is_sampleable_red_channel() {
        let (mut engine, _tex_id, sh_id, rid) = build_texture_and_shader_engine();
        engine.tick(500).expect("tick");

        let q = QueryKey::ProducedSlot {
            node: sh_id,
            slot: shader_output_path(),
        };
        resolve_with_engine_host(&mut engine, q, ResolveLogLevel::Off).expect("resolve");

        let texture = engine
            .render_texture_for_test(
                rid,
                &crate::render_product::RenderTextureRequest {
                    width: 8,
                    height: 8,
                    format: lps_shared::TextureStorageFormat::Rgba16Unorm,
                    time_seconds: 0.5,
                },
            )
            .expect("render texture");
        let batch = RenderSampleBatch {
            points: vec![RenderSamplePoint { x: 0.5, y: 0.5 }],
        };
        let sample = texture.sample_batch(&batch);
        assert!(sample.samples[0].color[0] > 0.4);
        assert!(sample.samples[0].color[0] < 0.6);
    }
}
