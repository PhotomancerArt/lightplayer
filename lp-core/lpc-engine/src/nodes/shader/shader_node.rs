//! Core shader node: compile GLSL via [`crate::gfx::LpGraphics`] and expose output as [`RuntimeProduct::Render`].

use alloc::boxed::Box;
use alloc::string::String;

use lpc_model::NodeId;
use lpc_model::Revision;
use lpc_model::SlotPath;
use lpc_model::nodes::shader::ShaderDef;

use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeResourceInitContext, NodeRuntime, PressureLevel,
    TickContext,
};
use crate::prop::ProducedSlotAccess;
use crate::render_product::{RenderProductId, ShaderRenderProduct};
use crate::runtime_product::RuntimeProduct;

/// Shader producer wired to the core engine; allocates a [`RenderProductId`] during [`NodeRuntime::init_resources`].
pub struct ShaderNode {
    node_id: NodeId,
    config: ShaderDef,
    glsl_source: String,
    render_product_id: RenderProductId,
    resources_initialized: bool,
    outputs: ShaderProducedSlots,
}

impl ShaderNode {
    pub fn new(node_id: NodeId, config: ShaderDef, glsl_source: String) -> Self {
        let dummy_id = RenderProductId::new(0);
        Self {
            node_id,
            config,
            glsl_source,
            render_product_id: dummy_id,
            resources_initialized: false,
            outputs: ShaderProducedSlots {
                path: shader_output_path(),
                render_product_id: dummy_id,
                last_frame: Revision::default(),
            },
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn render_product_id(&self) -> RenderProductId {
        self.render_product_id
    }
}

impl NodeRuntime for ShaderNode {
    fn init_resources(&mut self, ctx: &mut NodeResourceInitContext<'_>) -> Result<(), NodeError> {
        if self.resources_initialized {
            return Ok(());
        }
        let rid = ctx.insert_render_product(Box::new(ShaderRenderProduct::new(
            self.config.clone(),
            self.glsl_source.clone(),
        )));
        self.render_product_id = rid;
        self.outputs = ShaderProducedSlots {
            path: shader_output_path(),
            render_product_id: rid,
            last_frame: Revision::default(),
        };
        self.resources_initialized = true;
        Ok(())
    }

    fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        self.outputs.last_frame = ctx.revision();
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

    fn produced(&self) -> &dyn ProducedSlotAccess {
        &self.outputs
    }

    fn primary_render_product_id(&self) -> Option<RenderProductId> {
        self.resources_initialized.then_some(self.render_product_id)
    }
}

pub fn shader_output_path() -> SlotPath {
    SlotPath::parse("output").expect("shader output path")
}

#[derive(Clone)]
struct ShaderProducedSlots {
    path: SlotPath,
    render_product_id: RenderProductId,
    last_frame: Revision,
}

impl ProducedSlotAccess for ShaderProducedSlots {
    fn get(&self, path: &SlotPath) -> Option<(RuntimeProduct, Revision)> {
        if path == &self.path {
            Some((
                RuntimeProduct::render(self.render_product_id),
                self.last_frame,
            ))
        } else {
            None
        }
    }

    fn iter_changed_since<'a>(
        &'a self,
        since: Revision,
    ) -> Box<dyn Iterator<Item = (SlotPath, RuntimeProduct, Revision)> + 'a> {
        if self.last_frame.as_i64() > since.as_i64() {
            Box::new(core::iter::once((
                self.path.clone(),
                RuntimeProduct::render(self.render_product_id),
                self.last_frame,
            )))
        } else {
            Box::new(core::iter::empty())
        }
    }

    fn snapshot<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (SlotPath, RuntimeProduct, Revision)> + 'a> {
        Box::new(core::iter::once((
            self.path.clone(),
            RuntimeProduct::render(self.render_product_id),
            self.last_frame,
        )))
    }
}


#[cfg(test)]
mod tests {
    use alloc::sync::Arc;
    use alloc::vec;

    use super::*;
    use crate::engine::Engine;
    use crate::engine::resolve_with_engine_host;
    use crate::node::NodeResourceInitContext;
    use crate::node::test_placeholder_spine;
    use crate::nodes::TextureNode;
    use crate::render_product::{
        RenderProduct, RenderProductStore, RenderSampleBatch, RenderSamplePoint,
    };
    use crate::resolver::QueryKey;
    use crate::resolver::ResolveLogLevel;
    use crate::runtime_buffer::RuntimeBufferStore;
    use lpc_model::TreePath;
    use lpc_wire::{WireChildKind, WireSlotIndex};

    const DEMO_GLSL: &str = "layout(binding = 0) uniform vec2 outputSize; layout(binding = 1) uniform float time; vec4 render(vec2 pos) { return vec4(mod(time, 1.0), 0.0, 0.0, 1.0); }";

    fn build_texture_and_shader_engine() -> (Engine, NodeId, NodeId, RenderProductId) {
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

        let tex = TextureNode::new(tex_id, lpc_model::nodes::texture::TextureDef::new(8, 8));
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

        let rid = engine
            .primary_render_product_id_for_node(sh_id)
            .expect("shader render product id");

        (engine, tex_id, sh_id, rid)
    }

    #[test]
    fn shader_render_output_is_on_produced_slot_access() {
        let cfg = ShaderDef::default();
        let mut render_products = RenderProductStore::new();
        let mut runtime_buffers = RuntimeBufferStore::new();
        let mut ctx = NodeResourceInitContext::new(&mut render_products, &mut runtime_buffers);
        let mut node = ShaderNode::new(NodeId::new(1), cfg, String::new());
        node.init_resources(&mut ctx).expect("init resources");
        let rid = node.render_product_id();
        let p = shader_output_path();
        let (prod, _) = node.produced().get(&p).expect("render output");
        assert_eq!(prod.as_render(), Some(rid));
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

        let graphics = engine.graphics().cloned();
        let texture = engine
            .render_products_mut()
            .render_texture(
                rid,
                &crate::render_product::RenderTextureRequest {
                    width: 8,
                    height: 8,
                    format: lps_shared::TextureStorageFormat::Rgba16Unorm,
                    time_seconds: 0.5,
                },
                graphics.as_deref(),
            )
            .expect("render texture");
        let batch = RenderSampleBatch {
            points: vec![RenderSamplePoint { x: 0.5, y: 0.5 }],
        };
        let sample = texture.sample_batch(&batch).expect("sample");
        assert!(sample.samples[0].color[0] > 0.4);
        assert!(sample.samples[0].color[0] < 0.6);
    }
}
