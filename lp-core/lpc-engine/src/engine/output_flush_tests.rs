use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::sync::Arc;
use alloc::vec;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::dataflow::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
use crate::engine::default_demand_input_path;
use crate::engine::error::Error;
use crate::gfx::{LpGraphics, LpShader, ShaderCompileOptions};
use crate::node::test_placeholder_spine;
use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, RenderContext, RenderNode,
    TickContext,
};
use crate::nodes::{
    FixtureNode, OutputNode, fixture_input_path, output_input_path, shader_output_path,
};
use crate::products::visual::{RenderTextureRequest, TextureRenderProduct, VisualProduct};
use crate::resource::RuntimeBufferId;
use lpc_model::nodes::fixture::{ColorOrder, MappingConfig, PathSpec, RingOrder};
use lpc_model::nodes::output::OutputDef;
use lpc_model::{
    Dim2u, HardwareEndpointSpec, Kind, LpValue, Revision, ShaderState, SlotAccess, SlotPath,
    SlotShapeRegistry, SlotShapeRegistryError, StaticSlotShape, ToLpValue, TreePath,
};
use lpc_shared::output::{
    MemoryOutputProvider, OutputChannelHandle, OutputDriverOptions, OutputFormat, OutputProvider,
};
use lpc_wire::{WireChildKind, WireSlotIndex};

use super::{Engine, EngineServices};

#[derive(Clone)]
struct RcMemoryOutput(Rc<MemoryOutputProvider>);

impl OutputProvider for RcMemoryOutput {
    fn open(
        &self,
        endpoint: &HardwareEndpointSpec,
        byte_count: u32,
        format: OutputFormat,
        options: Option<OutputDriverOptions>,
    ) -> Result<OutputChannelHandle, lpc_shared::error::OutputError> {
        self.0.open(endpoint, byte_count, format, options)
    }

    fn write(
        &self,
        handle: OutputChannelHandle,
        data: &[u16],
    ) -> Result<(), lpc_shared::error::OutputError> {
        self.0.write(handle, data)
    }

    fn close(&self, handle: OutputChannelHandle) -> Result<(), lpc_shared::error::OutputError> {
        self.0.close(handle)
    }
}

fn endpoint(spec: &'static str) -> HardwareEndpointSpec {
    HardwareEndpointSpec::from_static(spec)
}

struct CountingGraphics {
    inner: crate::Graphics,
    output_alloc_count: AtomicU32,
    output_free_count: AtomicU32,
}

impl CountingGraphics {
    fn new() -> Self {
        Self {
            inner: crate::Graphics::new(),
            output_alloc_count: AtomicU32::new(0),
            output_free_count: AtomicU32::new(0),
        }
    }

    fn output_alloc_count(&self) -> u32 {
        self.output_alloc_count.load(Ordering::Relaxed)
    }

    fn output_free_count(&self) -> u32 {
        self.output_free_count.load(Ordering::Relaxed)
    }
}

impl LpGraphics for CountingGraphics {
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, Error> {
        self.inner.compile_shader(source, options)
    }

    fn backend_name(&self) -> &'static str {
        self.inner.backend_name()
    }

    fn alloc_output_buffer(
        &self,
        width: u32,
        height: u32,
    ) -> Result<lp_shader::LpsTextureBuf, Error> {
        self.output_alloc_count.fetch_add(1, Ordering::Relaxed);
        self.inner.alloc_output_buffer(width, height)
    }

    fn free_output_buffer(&self, buffer: lp_shader::LpsTextureBuf) {
        self.output_free_count.fetch_add(1, Ordering::Relaxed);
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

struct SolidFixtureProducer {
    state: ShaderState,
    ticks: Arc<AtomicU32>,
    color: [u16; 4],
}

impl NodeRuntime for SolidFixtureProducer {
    fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        self.ticks.fetch_add(1, Ordering::Relaxed);
        self.state
            .output
            .set_with_version(ctx.revision(), VisualProduct::new(ctx.node_id(), 0));
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

impl RenderNode for SolidFixtureProducer {
    fn render_texture(
        &mut self,
        _product: VisualProduct,
        request: &RenderTextureRequest,
        _ctx: &mut RenderContext<'_>,
    ) -> Result<TextureRenderProduct, NodeError> {
        solid_texture(request.width, request.height, request.format, self.color)
    }
}

fn solid_texture(
    width: u32,
    height: u32,
    format: lps_shared::TextureStorageFormat,
    color: [u16; 4],
) -> Result<TextureRenderProduct, NodeError> {
    let mut pixels = alloc::vec::Vec::new();
    let px_count = usize::try_from(width)
        .ok()
        .and_then(|w| usize::try_from(height).ok().map(|h| w.saturating_mul(h)))
        .ok_or_else(|| NodeError::msg("solid texture dimensions overflow"))?;
    for _ in 0..px_count {
        match format {
            lps_shared::TextureStorageFormat::Rgba16Unorm => {
                for c in color {
                    pixels.extend_from_slice(&c.to_le_bytes());
                }
            }
            lps_shared::TextureStorageFormat::Rgb16Unorm => {
                for c in [color[0], color[1], color[2]] {
                    pixels.extend_from_slice(&c.to_le_bytes());
                }
            }
            lps_shared::TextureStorageFormat::R16Unorm => {
                pixels.extend_from_slice(&color[0].to_le_bytes());
            }
        }
    }
    TextureRenderProduct::new(width, height, format, pixels)
        .map_err(|e| NodeError::msg(alloc::format!("solid texture: {e}")))
}

fn bind_fixture_def_defaults(rt: &mut Engine, fix_id: lpc_model::NodeId, frame: Revision) {
    bind_fixture_def_slot(
        rt,
        fix_id,
        frame,
        "render_size",
        Dim2u {
            width: 4,
            height: 4,
        }
        .to_lp_value(),
    );
    bind_fixture_def_slot(
        rt,
        fix_id,
        frame,
        "color_order",
        ColorOrder::Rgb.to_lp_value(),
    );
    bind_fixture_def_slot(rt, fix_id, frame, "brightness.some", LpValue::U32(255));
    bind_fixture_def_slot(
        rt,
        fix_id,
        frame,
        "gamma_correction.some",
        LpValue::Bool(false),
    );
}

fn bind_fixture_def_slot(
    rt: &mut Engine,
    fix_id: lpc_model::NodeId,
    frame: Revision,
    slot: &str,
    value: LpValue,
) {
    rt.add_binding(
        BindingDraft {
            source: BindingSource::Literal(value),
            target: BindingTarget::ConsumedSlot {
                node: fix_id,
                slot: SlotPath::parse(slot).unwrap(),
            },
            priority: BindingPriority::new(0),
            kind: Kind::Choice,
            owner: fix_id,
        },
        frame,
    )
    .unwrap();
}

fn attach_output_demand_root(
    rt: &mut Engine,
    root: lpc_model::NodeId,
    spine: lpc_model::NodeInvocation,
    artifact: crate::artifact::ArtifactId,
    frame: Revision,
    name: &str,
    endpoint: HardwareEndpointSpec,
) -> (lpc_model::NodeId, RuntimeBufferId) {
    let out_id = rt
        .tree_mut()
        .add_child(
            root,
            lpc_model::NodeName::parse(name).unwrap(),
            lpc_model::NodeName::parse("output").unwrap(),
            WireChildKind::Input {
                source: WireSlotIndex(0),
            },
            spine.clone(),
            artifact,
            frame,
        )
        .unwrap();

    rt.attach_runtime_node(out_id, Box::new(OutputNode::new()), frame)
        .unwrap();
    let sink = rt
        .runtime_output_sink_buffer_id(out_id)
        .expect("output sink buffer");
    rt.services_mut()
        .register_output_sink(sink, &OutputDef::new(endpoint));
    rt.add_binding(
        BindingDraft {
            source: BindingSource::Literal(LpValue::F32(0.0)),
            target: BindingTarget::ConsumedSlot {
                node: out_id,
                slot: default_demand_input_path(),
            },
            priority: BindingPriority::new(0),
            kind: Kind::Color,
            owner: out_id,
        },
        frame,
    )
    .unwrap();
    rt.add_demand_root(out_id);
    (out_id, sink)
}

fn attach_idle_output_sink(
    rt: &mut Engine,
    root: lpc_model::NodeId,
    spine: lpc_model::NodeInvocation,
    artifact: crate::artifact::ArtifactId,
    frame: Revision,
    name: &str,
    endpoint: HardwareEndpointSpec,
) -> (lpc_model::NodeId, RuntimeBufferId) {
    let out_id = rt
        .tree_mut()
        .add_child(
            root,
            lpc_model::NodeName::parse(name).unwrap(),
            lpc_model::NodeName::parse("output").unwrap(),
            WireChildKind::Input {
                source: WireSlotIndex(0),
            },
            spine.clone(),
            artifact,
            frame,
        )
        .unwrap();

    rt.attach_runtime_node(out_id, Box::new(OutputNode::new()), frame)
        .unwrap();
    let sink = rt
        .runtime_output_sink_buffer_id(out_id)
        .expect("output sink buffer");
    rt.services_mut()
        .register_output_sink(sink, &OutputDef::new(endpoint));
    (out_id, sink)
}

fn bind_output_to_fixture(
    rt: &mut Engine,
    out_id: lpc_model::NodeId,
    fix_id: lpc_model::NodeId,
    frame: Revision,
) {
    rt.add_binding(
        BindingDraft {
            source: BindingSource::ProducedSlot {
                node: fix_id,
                slot: SlotPath::parse("output").unwrap(),
            },
            target: BindingTarget::ConsumedSlot {
                node: out_id,
                slot: output_input_path(),
            },
            priority: BindingPriority::new(0),
            kind: Kind::Color,
            owner: out_id,
        },
        frame,
    )
    .unwrap();
}

#[test]
fn engine_output_sink_flush_writes_expected_rgb_via_memory_provider() {
    let mem = Rc::new(MemoryOutputProvider::new());
    let endpoint = endpoint("ws281x:rmt:D10");

    let path = TreePath::parse("/show.t").expect("path");
    let mut services = EngineServices::new(path.clone());
    services.set_output_provider(Some(Box::new(RcMemoryOutput(Rc::clone(&mem)))));
    let mut rt = Engine::with_services(path, services);
    let graphics = Arc::new(CountingGraphics::new());
    rt.set_graphics(Some(graphics.clone()));

    let ticks = Arc::new(AtomicU32::new(0));
    let frame = Revision::new(1);
    let root = rt.tree().root();
    let (spine, artifact) = test_placeholder_spine();

    let sh_id = rt
        .tree_mut()
        .add_child(
            root,
            lpc_model::NodeName::parse("sh").unwrap(),
            lpc_model::NodeName::parse("shader").unwrap(),
            WireChildKind::Input {
                source: WireSlotIndex(0),
            },
            spine.clone(),
            artifact,
            frame,
        )
        .unwrap();

    let out_path = shader_output_path();
    rt.attach_runtime_node(
        sh_id,
        Box::new(SolidFixtureProducer {
            state: ShaderState::new(VisualProduct::new(sh_id, 0)),
            ticks: Arc::clone(&ticks),
            color: [u16::MAX, 0, 0, u16::MAX],
        }),
        frame,
    )
    .unwrap();

    let mapping = MappingConfig::path_points_vec(
        vec![PathSpec::ring_array_counts(
            [0.5, 0.5],
            1.0,
            0,
            1,
            &[1],
            0.0,
            RingOrder::InnerFirst,
        )],
        2.0,
    );

    let fix_id = rt
        .tree_mut()
        .add_child(
            root,
            lpc_model::NodeName::parse("fx").unwrap(),
            lpc_model::NodeName::parse("fixture").unwrap(),
            WireChildKind::Input {
                source: WireSlotIndex(0),
            },
            spine.clone(),
            artifact,
            frame,
        )
        .unwrap();

    rt.attach_runtime_node(
        fix_id,
        Box::new(FixtureNode::new(
            fix_id,
            mapping,
            lpc_model::FixtureSamplingConfig::TextureArea,
            frame,
        )),
        frame,
    )
    .unwrap();
    bind_fixture_def_defaults(&mut rt, fix_id, frame);
    rt.add_binding(
        BindingDraft {
            source: BindingSource::ProducedSlot {
                node: sh_id,
                slot: out_path.clone(),
            },
            target: BindingTarget::ConsumedSlot {
                node: fix_id,
                slot: fixture_input_path(),
            },
            priority: BindingPriority::new(0),
            kind: Kind::Color,
            owner: fix_id,
        },
        frame,
    )
    .unwrap();

    let (out_id, _sink) = attach_output_demand_root(
        &mut rt,
        root,
        spine.clone(),
        artifact,
        frame,
        "out",
        endpoint.clone(),
    );
    bind_output_to_fixture(&mut rt, out_id, fix_id, frame);

    rt.tick(10).expect("tick");
    rt.tick(10)
        .expect("second tick reuses fixture render target");

    let handle = mem
        .get_handle_for_endpoint(&endpoint)
        .expect("channel opened");
    let got = mem.get_data(handle).expect("written");
    assert_eq!(got.len(), 3);
    assert_eq!(got[0], 65535);
    assert_eq!(got[1], 0);
    assert_eq!(got[2], 0);
    assert_eq!(
        graphics.output_alloc_count(),
        1,
        "fixture should allocate one render target and reuse it across frames",
    );
    assert_eq!(
        graphics.output_free_count(),
        0,
        "unchanged render size should not resize/free the fixture target",
    );
}

#[test]
fn engine_output_idle_registered_sink_skips_second_pin() {
    let mem = Rc::new(MemoryOutputProvider::new());
    let endpoint_written = endpoint("ws281x:rmt:D10");
    let endpoint_idle = endpoint("ws281x:rmt:GPIO19");

    let path = TreePath::parse("/show.t").expect("path");
    let mut services = EngineServices::new(path.clone());
    services.set_output_provider(Some(Box::new(RcMemoryOutput(Rc::clone(&mem)))));
    let mut rt = Engine::with_services(path, services);
    rt.set_graphics(Some(Arc::new(crate::Graphics::new())));

    let ticks = Arc::new(AtomicU32::new(0));
    let frame = Revision::new(1);
    let root = rt.tree().root();
    let (spine, artifact) = test_placeholder_spine();

    let sh_id = rt
        .tree_mut()
        .add_child(
            root,
            lpc_model::NodeName::parse("sh").unwrap(),
            lpc_model::NodeName::parse("shader").unwrap(),
            WireChildKind::Input {
                source: WireSlotIndex(0),
            },
            spine.clone(),
            artifact,
            frame,
        )
        .unwrap();

    let out_path = shader_output_path();
    rt.attach_runtime_node(
        sh_id,
        Box::new(SolidFixtureProducer {
            state: ShaderState::new(VisualProduct::new(sh_id, 0)),
            ticks: Arc::clone(&ticks),
            color: [u16::MAX, 0, 0, u16::MAX],
        }),
        frame,
    )
    .unwrap();

    let mapping = MappingConfig::path_points_vec(
        vec![PathSpec::ring_array_counts(
            [0.5, 0.5],
            1.0,
            0,
            1,
            &[1],
            0.0,
            RingOrder::InnerFirst,
        )],
        2.0,
    );

    let fix_id = rt
        .tree_mut()
        .add_child(
            root,
            lpc_model::NodeName::parse("fx").unwrap(),
            lpc_model::NodeName::parse("fixture").unwrap(),
            WireChildKind::Input {
                source: WireSlotIndex(0),
            },
            spine.clone(),
            artifact,
            frame,
        )
        .unwrap();

    rt.attach_runtime_node(
        fix_id,
        Box::new(FixtureNode::new(
            fix_id,
            mapping,
            lpc_model::FixtureSamplingConfig::TextureArea,
            frame,
        )),
        frame,
    )
    .unwrap();
    bind_fixture_def_defaults(&mut rt, fix_id, frame);
    rt.add_binding(
        BindingDraft {
            source: BindingSource::ProducedSlot {
                node: sh_id,
                slot: out_path.clone(),
            },
            target: BindingTarget::ConsumedSlot {
                node: fix_id,
                slot: fixture_input_path(),
            },
            priority: BindingPriority::new(0),
            kind: Kind::Color,
            owner: fix_id,
        },
        frame,
    )
    .unwrap();

    let (out_id, _sink_written) = attach_output_demand_root(
        &mut rt,
        root,
        spine.clone(),
        artifact,
        frame,
        "out_written",
        endpoint_written.clone(),
    );
    bind_output_to_fixture(&mut rt, out_id, fix_id, frame);
    let (_idle_out_id, _sink_idle) = attach_idle_output_sink(
        &mut rt,
        root,
        spine.clone(),
        artifact,
        frame,
        "out_idle",
        endpoint_idle.clone(),
    );

    rt.tick(10).expect("tick");

    assert!(
        mem.is_endpoint_open(&endpoint_written),
        "written sink should open its endpoint",
    );
    assert!(
        !mem.is_endpoint_open(&endpoint_idle),
        "idle sink buffer never updated this frame; should not flush or open",
    );
}

#[test]
fn output_demand_marks_output_buffer_dirty_same_frame_before_flush() {
    let mem = Rc::new(MemoryOutputProvider::new());
    let path = TreePath::parse("/show.t").expect("path");
    let mut services = EngineServices::new(path.clone());
    services.set_output_provider(Some(Box::new(RcMemoryOutput(Rc::clone(&mem)))));
    let mut rt = Engine::with_services(path, services);
    rt.set_graphics(Some(Arc::new(crate::Graphics::new())));

    let ticks = Arc::new(AtomicU32::new(0));
    let frame = Revision::new(1);
    let root = rt.tree().root();
    let (spine, artifact) = test_placeholder_spine();

    let sh_id = rt
        .tree_mut()
        .add_child(
            root,
            lpc_model::NodeName::parse("sh").unwrap(),
            lpc_model::NodeName::parse("shader").unwrap(),
            WireChildKind::Input {
                source: WireSlotIndex(0),
            },
            spine.clone(),
            artifact,
            frame,
        )
        .unwrap();

    let out_path = shader_output_path();
    rt.attach_runtime_node(
        sh_id,
        Box::new(SolidFixtureProducer {
            state: ShaderState::new(VisualProduct::new(sh_id, 0)),
            ticks: Arc::clone(&ticks),
            color: [0, u16::MAX, 0, u16::MAX],
        }),
        frame,
    )
    .unwrap();

    let mapping = MappingConfig::path_points_vec(
        vec![PathSpec::ring_array_counts(
            [0.5, 0.5],
            1.0,
            0,
            1,
            &[1],
            0.0,
            RingOrder::InnerFirst,
        )],
        2.0,
    );

    let fix_id = rt
        .tree_mut()
        .add_child(
            root,
            lpc_model::NodeName::parse("fx").unwrap(),
            lpc_model::NodeName::parse("fixture").unwrap(),
            WireChildKind::Input {
                source: WireSlotIndex(0),
            },
            spine.clone(),
            artifact,
            frame,
        )
        .unwrap();

    rt.attach_runtime_node(
        fix_id,
        Box::new(FixtureNode::new(
            fix_id,
            mapping,
            lpc_model::FixtureSamplingConfig::TextureArea,
            frame,
        )),
        frame,
    )
    .unwrap();
    bind_fixture_def_defaults(&mut rt, fix_id, frame);
    rt.add_binding(
        BindingDraft {
            source: BindingSource::ProducedSlot {
                node: sh_id,
                slot: out_path.clone(),
            },
            target: BindingTarget::ConsumedSlot {
                node: fix_id,
                slot: fixture_input_path(),
            },
            priority: BindingPriority::new(0),
            kind: Kind::Color,
            owner: fix_id,
        },
        frame,
    )
    .unwrap();

    let endpoint = endpoint("ws281x:rmt:D10");
    let (out_id, sink) = attach_output_demand_root(
        &mut rt,
        root,
        spine.clone(),
        artifact,
        frame,
        "out",
        endpoint.clone(),
    );
    bind_output_to_fixture(&mut rt, out_id, fix_id, frame);

    rt.tick(10).expect("tick");

    let ver_frame = rt.runtime_buffers().get(sink).expect("sink").changed_at();
    assert_eq!(
        ver_frame.as_i64(),
        rt.revision().as_i64(),
        "output demand should bump buffer version to current frame before flush runs",
    );

    let handle = mem.get_handle_for_endpoint(&endpoint).expect("opened");
    let got = mem.get_data(handle).expect("data");
    assert_eq!(got[1], 65535);
}
