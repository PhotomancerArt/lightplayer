//! Core shader node: compile GLSL via [`crate::gfx::LpGraphics`] and expose output as [`RuntimeProduct::Render`].

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec;

use lp_shader::LpsTextureBuf;
use lpc_model::FrameId;
use lpc_model::NodeId;
use lpc_model::prop::PropPath;
use lpc_model::prop::prop_path::parse_path;
use lpc_source::legacy::glsl_opts::{AddSubMode, DivMode, MulMode};
use lpc_source::legacy::nodes::shader::ShaderConfig;
use lps_shared::LpsValueF32;
use lps_shared::TextureBuffer;

use crate::gfx::{LpGraphics, LpShader, ShaderCompileOptions};
use crate::node::{
    DestroyCtx, MemPressureCtx, Node, NodeError, NodeResourceInitContext, PressureLevel,
    ShaderProjectionWire, TickContext,
};
use crate::prop::{RuntimeOutputAccess, RuntimePropAccess};
use crate::render_product::{RenderProductId, TextureRenderProduct};
use crate::resolver::QueryKey;
use crate::runtime_product::RuntimeProduct;

use super::texture_node::texture_dimension_query_targets;

/// Default max semantic errors forwarded from the GLSL → LPIR front-end (matches legacy shader runtime).
const SHADER_COMPILE_MAX_ERRORS: usize = 20;

pub fn shader_texture_output_path() -> PropPath {
    parse_path("texture").expect("texture output path")
}

/// Empty scalar props; shader texture output is only on [`ShaderRuntimeOutputs`].
#[derive(Clone, Copy)]
struct ShaderScalarProps;

impl RuntimePropAccess for ShaderScalarProps {
    fn get(&self, _path: &PropPath) -> Option<(LpsValueF32, FrameId)> {
        None
    }

    fn iter_changed_since<'a>(
        &'a self,
        _since: FrameId,
    ) -> Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'a> {
        Box::new(core::iter::empty())
    }

    fn snapshot<'a>(&'a self) -> Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'a> {
        Box::new(core::iter::empty())
    }
}

#[derive(Clone)]
struct ShaderRuntimeOutputs {
    path: PropPath,
    render_product_id: RenderProductId,
    last_frame: FrameId,
}

impl RuntimeOutputAccess for ShaderRuntimeOutputs {
    fn get(&self, path: &PropPath) -> Option<(RuntimeProduct, FrameId)> {
        if path == &self.path {
            Some((
                RuntimeProduct::render(self.render_product_id),
                self.last_frame,
            ))
        } else {
            None
        }
    }
}

/// Shader producer wired to the core engine; allocates a [`RenderProductId`] during [`Node::init_resources`].
pub struct ShaderNode {
    node_id: NodeId,
    texture_node_id: NodeId,
    config: ShaderConfig,
    glsl_source: String,
    /// Placeholder texture dimensions used until the first shader render read real texture props.
    placeholder_texture_width: u32,
    placeholder_texture_height: u32,
    render_product_id: RenderProductId,
    resources_initialized: bool,
    scalar_props: ShaderScalarProps,
    outputs: ShaderRuntimeOutputs,
    shader: Option<Box<dyn LpShader>>,
    output_buf: Option<LpsTextureBuf>,
    compilation_error: Option<String>,
}

impl ShaderNode {
    pub fn new(
        node_id: NodeId,
        texture_node_id: NodeId,
        config: ShaderConfig,
        glsl_source: String,
        placeholder_texture_width: u32,
        placeholder_texture_height: u32,
    ) -> Self {
        let dummy_id = RenderProductId::new(0);
        Self {
            node_id,
            texture_node_id,
            config,
            glsl_source,
            placeholder_texture_width,
            placeholder_texture_height,
            render_product_id: dummy_id,
            resources_initialized: false,
            scalar_props: ShaderScalarProps,
            outputs: ShaderRuntimeOutputs {
                path: shader_texture_output_path(),
                render_product_id: dummy_id,
                last_frame: FrameId::default(),
            },
            shader: None,
            output_buf: None,
            compilation_error: None,
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn render_product_id(&self) -> RenderProductId {
        self.render_product_id
    }

    pub fn compilation_error(&self) -> Option<&str> {
        self.compilation_error.as_deref()
    }

    fn build_placeholder_texture_product(&self) -> Result<TextureRenderProduct, NodeError> {
        let len = rgba16_placeholder_byte_len(
            self.placeholder_texture_width,
            self.placeholder_texture_height,
        )?;
        TextureRenderProduct::rgba16_unorm(
            self.placeholder_texture_width,
            self.placeholder_texture_height,
            vec![0u8; len],
        )
        .map_err(|e| NodeError::msg(format!("create placeholder texture product: {e}")))
    }
}

impl Node for ShaderNode {
    fn init_resources(&mut self, ctx: &mut NodeResourceInitContext<'_>) -> Result<(), NodeError> {
        if self.resources_initialized {
            return Ok(());
        }
        let tex = Box::new(self.build_placeholder_texture_product()?);
        let rid = ctx.insert_render_product(tex);
        self.render_product_id = rid;
        self.outputs = ShaderRuntimeOutputs {
            path: shader_texture_output_path(),
            render_product_id: rid,
            last_frame: FrameId::default(),
        };
        self.resources_initialized = true;
        Ok(())
    }
    fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        if self.shader.is_none() && self.compilation_error.is_none() {
            let g = ctx
                .graphics()
                .ok_or_else(|| NodeError::msg("Engine has no graphics; cannot run ShaderNode"))?;
            let _ = self.compile(g);
        }

        let Some(shader) = self.shader.as_mut() else {
            self.outputs.last_frame = ctx.frame_id();
            return Ok(());
        };

        if !shader.has_render() {
            return Err(NodeError::msg("compiled shader has no render() entry"));
        }

        let (tn, wpath, hpath) = texture_dimension_query_targets(self.texture_node_id);
        let w_prod = ctx
            .resolve(QueryKey::NodeInput {
                node: tn,
                input: wpath,
            })
            .map_err(|e| NodeError::msg(format!("resolve texture width: {}", e.message)))?;
        let h_prod = ctx
            .resolve(QueryKey::NodeInput {
                node: tn,
                input: hpath,
            })
            .map_err(|e| NodeError::msg(format!("resolve texture height: {}", e.message)))?;

        let width = match w_prod.as_value() {
            Some(LpsValueF32::I32(v)) if *v > 0 => *v as u32,
            Some(LpsValueF32::U32(v)) if *v > 0 => *v,
            _ => {
                return Err(NodeError::msg(
                    "texture width missing or invalid (expected positive I32/U32)",
                ));
            }
        };
        let height = match h_prod.as_value() {
            Some(LpsValueF32::I32(v)) if *v > 0 => *v as u32,
            Some(LpsValueF32::U32(v)) if *v > 0 => *v,
            _ => {
                return Err(NodeError::msg(
                    "texture height missing or invalid (expected positive I32/U32)",
                ));
            }
        };

        let graphics = ctx
            .graphics()
            .ok_or_else(|| NodeError::msg("Engine has no graphics; cannot run ShaderNode"))?;

        let need_alloc = match &self.output_buf {
            None => true,
            Some(buf) => buf.width() != width || buf.height() != height,
        };
        if need_alloc {
            self.output_buf = Some(
                graphics
                    .alloc_output_buffer(width, height)
                    .map_err(|e| NodeError::msg(format!("alloc_output_buffer: {e}")))?,
            );
        }

        let buf = self
            .output_buf
            .as_mut()
            .ok_or_else(|| NodeError::msg("internal: output buffer missing after alloc"))?;

        shader
            .render(buf, ctx.time_seconds())
            .map_err(|e| NodeError::msg(format!("shader render: {e}")))?;

        let pixels = buf.data().to_vec();
        let tex = TextureRenderProduct::new(width, height, buf.format(), pixels)
            .map_err(|e| NodeError::msg(format!("texture product: {e}")))?;

        ctx.defer_render_product_replace(self.render_product_id, Box::new(tex))?;
        self.outputs.last_frame = ctx.frame_id();

        Ok(())
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError> {
        self.shader = None;
        self.output_buf = None;
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: PressureLevel,
        _ctx: &mut MemPressureCtx<'_>,
    ) -> Result<(), NodeError> {
        self.shader = None;
        Ok(())
    }

    fn props(&self) -> &dyn RuntimePropAccess {
        &self.scalar_props
    }

    fn outputs(&self) -> &dyn RuntimeOutputAccess {
        &self.outputs
    }

    fn primary_render_product_id(&self) -> Option<RenderProductId> {
        self.resources_initialized.then_some(self.render_product_id)
    }

    fn shader_projection_wire(&self) -> Option<ShaderProjectionWire<'_>> {
        Some(ShaderProjectionWire {
            glsl_source: self.glsl_source.as_str(),
            compilation_error: self.compilation_error.as_deref(),
            render_product_id: self.resources_initialized.then_some(self.render_product_id),
        })
    }
}

impl ShaderNode {
    fn compile(&mut self, graphics: &dyn LpGraphics) -> Result<(), NodeError> {
        self.compilation_error = None;
        let q32_options = map_model_q32_options(&self.config.glsl_opts);
        let compile_opts = ShaderCompileOptions {
            q32_options,
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

        match compile_result {
            Ok(s) => {
                self.shader = Some(s);
                Ok(())
            }
            Err(e) => {
                self.compilation_error = Some(e.clone());
                self.shader = None;
                Err(NodeError::msg(format!("shader compile: {e}")))
            }
        }
    }
}

fn map_model_q32_options(
    opts: &lpc_source::legacy::glsl_opts::GlslOpts,
) -> lps_q32::q32_options::Q32Options {
    lps_q32::q32_options::Q32Options {
        add_sub: match opts.add_sub {
            AddSubMode::Saturating => lps_q32::q32_options::AddSubMode::Saturating,
            AddSubMode::Wrapping => lps_q32::q32_options::AddSubMode::Wrapping,
        },
        mul: match opts.mul {
            MulMode::Saturating => lps_q32::q32_options::MulMode::Saturating,
            MulMode::Wrapping => lps_q32::q32_options::MulMode::Wrapping,
        },
        div: match opts.div {
            DivMode::Saturating => lps_q32::q32_options::DivMode::Saturating,
            DivMode::Reciprocal => lps_q32::q32_options::DivMode::Reciprocal,
        },
    }
}

fn rgba16_placeholder_byte_len(width: u32, height: u32) -> Result<usize, NodeError> {
    usize::try_from(width)
        .ok()
        .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)))
        .and_then(|px| px.checked_mul(8))
        .ok_or_else(|| {
            NodeError::msg(format!(
                "shader placeholder texture dimensions {width}x{height} overflow usize"
            ))
        })
}

#[cfg(test)]
mod tests {
    use alloc::sync::Arc;
    use alloc::vec;

    use super::super::TextureNode;
    use super::*;
    use crate::engine::Engine;
    use crate::engine::resolve_with_engine_host;
    use crate::node::NodeResourceInitContext;
    use crate::render_product::{RenderProductStore, RenderSampleBatch, RenderSamplePoint};
    use crate::resolver::ResolveLogLevel;
    use crate::runtime_buffer::RuntimeBufferStore;
    use crate::tree::test_placeholder_spine;
    use lpc_model::TreePath;
    use lpc_wire::{WireChildKind, WireSlotIndex};

    const DEMO_GLSL: &str = "layout(binding = 0) uniform vec2 outputSize; layout(binding = 1) uniform float time; vec4 render(vec2 pos) { return vec4(mod(time, 1.0), 0.0, 0.0, 1.0); }";

    fn build_texture_and_shader_engine() -> (Engine, NodeId, NodeId, RenderProductId) {
        let mut engine = Engine::new(TreePath::parse("/show.t").expect("path"));
        engine.set_graphics(Some(Arc::new(crate::Graphics::new())));
        let frame = FrameId::new(1);
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

        let tex = TextureNode::new(
            tex_id,
            lpc_source::legacy::nodes::texture::TextureConfig {
                width: 8,
                height: 8,
            },
        );
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

        let cfg = ShaderConfig {
            glsl_path: lpc_model::LpPathBuf::from("main.glsl"),
            texture_spec: lpc_model::NodeSpec::from(""),
            render_order: 0,
            glsl_opts: Default::default(),
        };

        let sh = ShaderNode::new(sh_id, tex_id, cfg, String::from(DEMO_GLSL), 8, 8);
        engine
            .attach_runtime_node(sh_id, Box::new(sh), frame)
            .expect("attach shader");

        let rid = engine
            .primary_render_product_id_for_node(sh_id)
            .expect("shader render product id");

        (engine, tex_id, sh_id, rid)
    }

    #[test]
    fn shader_render_output_is_on_runtime_output_access_only() {
        let cfg = ShaderConfig {
            glsl_path: lpc_model::LpPathBuf::from("main.glsl"),
            texture_spec: lpc_model::NodeSpec::from(""),
            render_order: 0,
            glsl_opts: Default::default(),
        };
        let mut render_products = RenderProductStore::new();
        let mut runtime_buffers = RuntimeBufferStore::new();
        let mut ctx = NodeResourceInitContext::new(&mut render_products, &mut runtime_buffers);
        let mut node = ShaderNode::new(NodeId::new(1), NodeId::new(2), cfg, String::new(), 8, 8);
        node.init_resources(&mut ctx).expect("init resources");
        let rid = node.render_product_id();
        let p = shader_texture_output_path();
        assert!(node.props().get(&p).is_none());
        let (prod, _) = node.outputs().get(&p).expect("render output");
        assert_eq!(prod.as_render(), Some(rid));
    }

    #[test]
    fn shader_core_produces_render_runtime_product() {
        let (mut engine, _tex_id, sh_id, rid) = build_texture_and_shader_engine();
        engine.tick(1000).expect("tick");

        let q = QueryKey::NodeOutput {
            node: sh_id,
            output: shader_texture_output_path(),
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

        let q = QueryKey::NodeOutput {
            node: sh_id,
            output: shader_texture_output_path(),
        };
        resolve_with_engine_host(&mut engine, q, ResolveLogLevel::Off).expect("resolve");

        let batch = RenderSampleBatch {
            points: vec![RenderSamplePoint { x: 0.5, y: 0.5 }],
        };
        let sample = engine
            .render_products()
            .sample_batch(rid, &batch)
            .expect("sample");
        assert!(sample.samples[0].color[0] > 0.4);
        assert!(sample.samples[0].color[0] < 0.6);
    }
}
