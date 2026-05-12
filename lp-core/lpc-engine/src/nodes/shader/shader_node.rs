//! Core shader node: owns GLSL compilation/rendering and exposes output as a visual product value.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;

use lpc_model::{
    AddSubMode, DivMode, GlslOpts, MulMode, NodeId, ShaderState, SlotAccess, SlotPath,
    SlotShapeRegistry, SlotShapeRegistryError, StaticSlotShape,
};
use lpc_model::{ShaderDef, SlotAccessor};
use lps_shared::TextureBuffer;

use crate::gfx::{LpShader, ShaderCompileOptions};
use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, RenderContext, RenderNode,
    TickContext,
};
use crate::products::visual::{RenderTextureRequest, TextureRenderProduct, VisualProduct};
use crate::products::visual::{VisualSampleBufferRequest, VisualSampleTarget};
/// Default max semantic errors forwarded from the GLSL to LPIR front end.
const SHADER_COMPILE_MAX_ERRORS: usize = 20;

/// Shader producer wired to the core engine.
pub struct ShaderNode {
    node_id: NodeId,
    glsl_source: String,
    glsl_opts: GlslOpts,
    config_accessors: Option<ShaderConfigAccessors>,
    shader: Option<Box<dyn LpShader>>,
    compilation_error: Option<String>,
    state: ShaderState,
}

impl ShaderNode {
    pub fn new(node_id: NodeId, glsl_source: String) -> Self {
        Self {
            node_id,
            glsl_source,
            glsl_opts: GlslOpts::default(),
            config_accessors: None,
            shader: None,
            compilation_error: None,
            state: ShaderState::new(VisualProduct::new(node_id, 0)),
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn visual_product(&self) -> VisualProduct {
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
            q32_options: map_model_q32_options(&self.glsl_opts),
            max_errors: Some(SHADER_COMPILE_MAX_ERRORS),
        };

        let compile_start_ms = ctx.now_ms();
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
        let compile_elapsed_ms = compile_start_ms.and_then(|start| ctx.elapsed_ms(start));
        lp_perf::emit_end!(lp_perf::EVENT_SHADER_COMPILE);

        match compile_result {
            Ok(shader) => {
                self.shader = Some(shader);
                if let Some(compile_elapsed_ms) = compile_elapsed_ms {
                    log::info!(
                        "[shader-node] compilation succeeded (node={:?}, elapsed={}ms)",
                        self.node_id,
                        compile_elapsed_ms
                    );
                } else {
                    log::info!(
                        "[shader-node] compilation succeeded (node={:?})",
                        self.node_id
                    );
                }
                Ok(())
            }
            Err(error) => {
                self.compilation_error = Some(error.clone());
                self.shader = None;
                if let Some(compile_elapsed_ms) = compile_elapsed_ms {
                    log::warn!(
                        "[shader-node] compilation failed (node={:?}, elapsed={}ms): {error}",
                        self.node_id,
                        compile_elapsed_ms
                    );
                } else {
                    log::warn!(
                        "[shader-node] compilation failed (node={:?}): {error}",
                        self.node_id
                    );
                }
                Err(NodeError::msg(format!("shader compile: {error}")))
            }
        }
    }

    fn update_config_from_view(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let accessors =
            ShaderConfigAccessors::get_or_compile(&mut self.config_accessors, ctx.slot_shapes())
                .map_err(|e| NodeError::msg(format!("compile shader config view: {e}")))?;
        let next_add_sub = accessors.add_sub.get(ctx)?;
        let next_mul = accessors.mul.get(ctx)?;
        let next_div = accessors.div.get(ctx)?;
        if *self.glsl_opts.add_sub.value() != next_add_sub
            || *self.glsl_opts.mul.value() != next_mul
            || *self.glsl_opts.div.value() != next_div
        {
            self.glsl_opts = GlslOpts {
                add_sub: lpc_model::ValueSlot::with_version(ctx.revision(), next_add_sub),
                mul: lpc_model::ValueSlot::with_version(ctx.revision(), next_mul),
                div: lpc_model::ValueSlot::with_version(ctx.revision(), next_div),
            };
            self.shader = None;
        }
        Ok(())
    }
}

impl NodeRuntime for ShaderNode {
    fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        self.update_config_from_view(ctx)?;
        self.state
            .output
            .set_with_version(ctx.revision(), VisualProduct::new(self.node_id, 0));
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

    fn runtime_state_slots(&self) -> Option<&dyn SlotAccess> {
        Some(&self.state)
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

struct ShaderConfigAccessors {
    registry_revision: lpc_model::Revision,
    add_sub: SlotAccessor,
    mul: SlotAccessor,
    div: SlotAccessor,
}

impl ShaderConfigAccessors {
    fn compile(registry: &SlotShapeRegistry) -> Result<Self, lpc_model::SlotAccessorError> {
        Ok(Self {
            registry_revision: registry.revision(),
            add_sub: compile_shader_config_value_accessor("glsl_opts.add_sub", registry)?,
            mul: compile_shader_config_value_accessor("glsl_opts.mul", registry)?,
            div: compile_shader_config_value_accessor("glsl_opts.div", registry)?,
        })
    }

    fn get_or_compile<'a>(
        cache: &'a mut Option<Self>,
        registry: &SlotShapeRegistry,
    ) -> Result<&'a Self, lpc_model::SlotAccessorError> {
        let needs_compile = cache
            .as_ref()
            .is_none_or(|view| view.registry_revision != registry.revision());
        if needs_compile {
            *cache = Some(Self::compile(registry)?);
        }
        Ok(cache
            .as_ref()
            .expect("shader config accessors were just compiled"))
    }
}

fn compile_shader_config_value_accessor(
    path: &str,
    registry: &SlotShapeRegistry,
) -> Result<SlotAccessor, lpc_model::SlotAccessorError> {
    SlotAccessor::compile_value(
        ShaderDef::SHAPE_ID,
        SlotPath::parse(path).expect("shader config accessor path is valid"),
        registry,
    )
}

pub fn shader_output_path() -> SlotPath {
    SlotPath::parse("output").expect("shader output path")
}

impl RenderNode for ShaderNode {
    fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        ctx: &mut RenderContext<'_>,
    ) -> Result<TextureRenderProduct, NodeError> {
        let mut texture = {
            let graphics = ctx
                .graphics()
                .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
            let texture = graphics
                .alloc_output_buffer(request.width, request.height)
                .map_err(|e| NodeError::msg(format!("alloc_output_buffer: {e}")))?;
            if texture.format() != request.format {
                let allocated = texture.format();
                graphics.free_output_buffer(texture);
                return Err(NodeError::msg(format!(
                    "graphics allocated {allocated:?}, requested {:?}",
                    request.format
                )));
            }
            texture
        };
        if let Err(e) = self.render_texture_into(product, request, &mut texture, ctx) {
            if let Some(graphics) = ctx.graphics() {
                graphics.free_output_buffer(texture);
            }
            return Err(e);
        }

        let width = texture.width();
        let height = texture.height();
        let format = texture.format();
        let pixels = texture.data().to_vec();
        if let Some(graphics) = ctx.graphics() {
            graphics.free_output_buffer(texture);
        }

        TextureRenderProduct::new(width, height, format, pixels)
            .map_err(|e| NodeError::msg(format!("texture product: {e}")))
    }

    fn render_texture_into(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        target: &mut lp_shader::LpsTextureBuf,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        validate_shader_visual_product(self.node_id, product)?;
        if target.width() != request.width
            || target.height() != request.height
            || target.format() != request.format
        {
            return Err(NodeError::msg(format!(
                "shader render target {:?} {}x{} does not match request {:?} {}x{}",
                target.format(),
                target.width(),
                target.height(),
                request.format,
                request.width,
                request.height
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
        shader
            .render(target, request.time_seconds)
            .map_err(|e| NodeError::msg(format!("shader render: {e}")))
    }

    fn sample_visual_into(
        &mut self,
        product: VisualProduct,
        request: VisualSampleBufferRequest<'_>,
        target: VisualSampleTarget<'_>,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        validate_shader_visual_product(self.node_id, product)?;
        if target.samples.count() != request.points.count() {
            return Err(NodeError::msg(format!(
                "shader sample target count {} does not match request count {}",
                target.samples.count(),
                request.points.count()
            )));
        }

        self.ensure_compiled(ctx)?;
        let shader = self
            .shader
            .as_mut()
            .ok_or_else(|| NodeError::msg("shader missing after compile"))?;
        shader
            .sample_rgba16(
                request.points,
                target.samples,
                request.output_width,
                request.output_height,
                request.time_seconds,
            )
            .map_err(|e| NodeError::msg(format!("shader sample: {e}")))
    }
}

fn validate_shader_visual_product(
    node_id: lpc_model::NodeId,
    product: VisualProduct,
) -> Result<(), NodeError> {
    if product.node() != node_id {
        return Err(NodeError::msg(format!(
            "shader node {node_id:?} cannot render visual product owned by {:?}",
            product.node()
        )));
    }
    if product.output() != 0 {
        return Err(NodeError::msg(format!(
            "shader node {node_id:?} has no render output {}",
            product.output()
        )));
    }
    Ok(())
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
    use core::sync::atomic::{AtomicU32, Ordering};

    use super::*;
    use crate::artifact::ArtifactLocation;
    use crate::dataflow::resolver::QueryKey;
    use crate::dataflow::resolver::ResolveLogLevel;
    use crate::engine::Engine;
    use crate::engine::error::Error;
    use crate::engine::resolve_with_engine_host;
    use crate::gfx::LpGraphics;
    use crate::nodes::TextureNode;
    use crate::products::visual::{VisualProduct, VisualSampleBatch, VisualSamplePoint};
    use lpc_model::{
        ArtifactLocator, NodeDef, NodeInvocation, Revision, SlotDataAccess, StaticSlotShape,
        TextureDef, TreePath,
    };
    use lpc_wire::{WireChildKind, WireSlotIndex};

    const DEMO_GLSL: &str = "layout(binding = 0) uniform vec2 outputSize; layout(binding = 1) uniform float time; vec4 render(vec2 pos) { return vec4(mod(time, 1.0), 0.0, 0.0, 1.0); }";

    fn build_texture_and_shader_engine() -> (Engine, NodeId, NodeId, VisualProduct) {
        let mut engine = Engine::new(TreePath::parse("/show.t").expect("path"));
        engine.set_graphics(Some(Arc::new(crate::Graphics::new())));
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let tex_invocation = NodeInvocation::new(ArtifactLocator::path("tex.toml"));
        let tex_artifact = engine
            .artifacts_mut()
            .acquire_location(ArtifactLocation::file("tex.toml"), frame);
        engine
            .artifacts_mut()
            .load_with(&tex_artifact, frame, |_| {
                Ok(NodeDef::Texture(TextureDef::new(8, 8)))
            })
            .expect("load texture artifact");
        let shader_invocation = NodeInvocation::new(ArtifactLocator::path("shader.toml"));
        let shader_artifact = engine
            .artifacts_mut()
            .acquire_location(ArtifactLocation::file("shader.toml"), frame);
        engine
            .artifacts_mut()
            .load_with(&shader_artifact, frame, |_| {
                Ok(NodeDef::Shader(ShaderDef::default()))
            })
            .expect("load shader artifact");

        let tex_id = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("tex").expect("name"),
                lpc_model::NodeName::parse("texture").expect("ty"),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                tex_invocation,
                tex_artifact,
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
                shader_invocation,
                shader_artifact,
                frame,
            )
            .expect("shader");

        let sh = ShaderNode::new(sh_id, String::from(DEMO_GLSL));
        engine
            .attach_runtime_node(sh_id, Box::new(sh), frame)
            .expect("attach shader");

        let rid = VisualProduct::new(sh_id, 0);

        (engine, tex_id, sh_id, rid)
    }

    #[test]
    fn shader_render_output_is_on_runtime_state_slot_root() {
        let node = ShaderNode::new(NodeId::new(1), String::new());

        let state = node.runtime_state_slots().expect("shader state slots");
        assert_eq!(state.shape_id(), ShaderState::SHAPE_ID);
        let SlotDataAccess::Record(record) = state.data() else {
            panic!("shader runtime state should be a record");
        };
        let Some(SlotDataAccess::Value(output)) = record.field(0) else {
            panic!("shader runtime state output should be a value");
        };

        assert_eq!(
            output.value(),
            lpc_model::LpValue::Product(lpc_model::ProductRef::visual(node.visual_product()))
        );
    }

    #[test]
    fn shader_core_produces_visual_product_value() {
        let (mut engine, _tex_id, sh_id, rid) = build_texture_and_shader_engine();
        engine.tick(1000).expect("tick");

        let q = QueryKey::ProducedSlot {
            node: sh_id,
            slot: shader_output_path(),
        };
        let prod = resolve_with_engine_host(&mut engine, q, ResolveLogLevel::Off)
            .expect("resolve")
            .0;
        let got_id = match prod.product.get() {
            lpc_model::LpValue::Product(lpc_model::ProductRef::Visual(product)) => *product,
            other => panic!("expected visual product, got {other:?}"),
        };
        assert_eq!(got_id, rid);
    }

    #[test]
    fn shader_core_visual_product_is_sampleable_red_channel() {
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
                &crate::products::visual::RenderTextureRequest {
                    width: 8,
                    height: 8,
                    format: lps_shared::TextureStorageFormat::Rgba16Unorm,
                    time_seconds: 0.5,
                },
            )
            .expect("render texture");
        let batch = VisualSampleBatch {
            points: vec![VisualSamplePoint {
                x_q16: 32768,
                y_q16: 32768,
            }],
            time_seconds: 0.5,
        };
        let sample = texture.sample_batch(&batch);
        assert!(sample.samples[0].rgba_unorm16[0] > 26_000);
        assert!(sample.samples[0].rgba_unorm16[0] < 40_000);
    }

    #[test]
    fn shader_compile_cache_survives_unchanged_config_across_frames() {
        let (mut engine, _tex_id, sh_id, rid) = build_texture_and_shader_engine();
        let graphics = Arc::new(CountingGraphics::new());
        engine.set_graphics(Some(graphics.clone()));

        for time_ms in [500, 600, 700] {
            engine.tick(time_ms).expect("tick");
            resolve_with_engine_host(
                &mut engine,
                QueryKey::ProducedSlot {
                    node: sh_id,
                    slot: shader_output_path(),
                },
                ResolveLogLevel::Off,
            )
            .expect("resolve");
            engine
                .render_texture_for_test(
                    rid,
                    &crate::products::visual::RenderTextureRequest {
                        width: 8,
                        height: 8,
                        format: lps_shared::TextureStorageFormat::Rgba16Unorm,
                        time_seconds: time_ms as f32 / 1000.0,
                    },
                )
                .expect("render texture");
        }

        assert_eq!(graphics.compile_count(), 1);
    }

    struct CountingGraphics {
        inner: crate::Graphics,
        compile_count: AtomicU32,
    }

    impl CountingGraphics {
        fn new() -> Self {
            Self {
                inner: crate::Graphics::new(),
                compile_count: AtomicU32::new(0),
            }
        }

        fn compile_count(&self) -> u32 {
            self.compile_count.load(Ordering::Relaxed)
        }
    }

    impl LpGraphics for CountingGraphics {
        fn compile_shader(
            &self,
            _source: &str,
            _options: &ShaderCompileOptions,
        ) -> Result<Box<dyn LpShader>, Error> {
            self.compile_count.fetch_add(1, Ordering::Relaxed);
            Ok(Box::new(CountingShader))
        }

        fn backend_name(&self) -> &'static str {
            "counting-test"
        }

        fn alloc_output_buffer(
            &self,
            width: u32,
            height: u32,
        ) -> Result<lp_shader::LpsTextureBuf, Error> {
            self.inner.alloc_output_buffer(width, height)
        }

        fn free_output_buffer(&self, buffer: lp_shader::LpsTextureBuf) {
            self.inner.free_output_buffer(buffer);
        }

        fn alloc_sample_points(&self, count: u32) -> Result<lp_shader::LpsSamplePointBuf, Error> {
            self.inner.alloc_sample_points(count)
        }

        fn alloc_sample_rgba16(&self, count: u32) -> Result<lp_shader::LpsSampleRgba16Buf, Error> {
            self.inner.alloc_sample_rgba16(count)
        }

        fn free_sample_points(&self, buffer: lp_shader::LpsSamplePointBuf) {
            self.inner.free_sample_points(buffer);
        }

        fn free_sample_rgba16(&self, buffer: lp_shader::LpsSampleRgba16Buf) {
            self.inner.free_sample_rgba16(buffer);
        }
    }

    struct CountingShader;

    impl LpShader for CountingShader {
        fn render(
            &mut self,
            texture: &mut lp_shader::LpsTextureBuf,
            _time: f32,
        ) -> Result<(), Error> {
            texture.data_mut().fill(0);
            Ok(())
        }

        fn has_render(&self) -> bool {
            true
        }
    }
}
