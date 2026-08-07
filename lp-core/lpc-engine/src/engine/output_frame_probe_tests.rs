//! Host tests for the published output-frame read.
//!
//! The claim under test is narrow and load-bearing: a client can read the
//! exact bytes an output already pushed, with usable layout metadata, and
//! WITHOUT the device rendering anything for the request. Every test here
//! builds the same shader → fixture → output chain, ticks it, and then reads
//! through the real `ProjectRead` stream (chunk reassembly included) rather
//! than calling the engine method directly.

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use lpc_model::nodes::fixture::{ColorOrder, MappingConfig, PathSpec};
use lpc_model::{
    ControlDisplayLayout, Dim2u, Kind, LpValue, NodeId, Revision, ShaderState, SlotAccess,
    SlotPath, SlotShapeRegistry, SlotShapeRegistryError, ToLpValue, TreePath,
};
use lpc_registry::ProjectRegistry;
use lpc_wire::{
    ControlDisplayLayoutProbeResult, ControlDisplayLayoutRead, OutputFrameEntry,
    OutputFrameProbeRequest, OutputFrameProbeResult, ProjectProbeRequest, ProjectProbeResult,
    ProjectReadRequest, WireChannelSampleFormat, WireChildKind, WireSlotIndex,
};

use crate::dataflow::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
use crate::engine::test_support::read_probe_results;
use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, ProduceResult,
    RenderContext, RenderNode, RuntimeStateShape, TickContext, test_placeholder_spine,
};
use crate::nodes::{
    FixtureNode, OutputNode, fixture_input_path, output_input_path, shader_output_path,
};
use crate::products::visual::{RenderTextureRequest, TextureRenderProduct, VisualProduct};
use crate::resource::RuntimeBufferId;

use super::Engine;

/// The published bytes are the buffer's bytes, verbatim, and they arrive with
/// interpretation metadata — and the read renders nothing.
///
/// The render counter is the actual assertion about cost: the control-product
/// probe would bump it (it re-renders the fixture, which pulls the shader),
/// and this read must not.
#[test]
fn output_frame_probe_returns_published_bytes_without_rendering() {
    let mut harness = Harness::build([u16::MAX, 0, 0, u16::MAX]);
    harness.tick();

    let published = harness.published_bytes();
    assert_eq!(published, vec![255, 255, 0, 0, 0, 0], "red lamp, u16 LE");

    let renders_before = harness.renders();
    let entries = harness.read(ControlDisplayLayoutRead::Always);
    assert_eq!(
        harness.renders(),
        renders_before,
        "the published-frame read must not render"
    );

    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.node, harness.out_id);
    assert_eq!(entry.bytes, published, "bytes must be the buffer verbatim");
    // `channels` is the buffer's own count: RGB lamps, not raw samples.
    assert_eq!(entry.channels, 1);
    assert_eq!(entry.sample_format, WireChannelSampleFormat::U16);

    assert_eq!(entry.sample_layout.spans.len(), 1);
    assert_eq!(entry.sample_layout.spans[0].len, 3);

    let ControlDisplayLayoutProbeResult::Layout(ControlDisplayLayout::Layout2d(layout)) =
        &entry.display_layout
    else {
        panic!(
            "expected a 2D display layout, got {:?}",
            entry.display_layout
        );
    };
    assert_eq!(layout.lamps.len(), 1);
    assert_eq!(layout.lamps[0].sample_start, 0);
    assert_eq!((layout.width_hint, layout.height_hint), (4, 4));
}

/// Change detection: a second tick republishes, and the entry's revision
/// moves. This is the whole signal a card feed keys on — "is this a new
/// frame?" — so it is asserted against a real second publish, not a stub.
#[test]
fn output_frame_probe_revision_moves_on_each_publish() {
    let mut harness = Harness::build([u16::MAX, 0, 0, u16::MAX]);
    harness.tick();
    let first = harness.read(ControlDisplayLayoutRead::None)[0].revision;

    harness.tick();
    let second = harness.read(ControlDisplayLayoutRead::None)[0].revision;

    assert!(
        second > first,
        "republished frame must advance the revision ({first:?} -> {second:?})"
    );

    // And a read that does not tick in between sees the same revision — the
    // read itself must not look like a new frame.
    let repeat = harness.read(ControlDisplayLayoutRead::None)[0].revision;
    assert_eq!(repeat, second, "a re-read is not a new frame");
}

/// Geometry gating: `IfChanged` with the layout's own revision answers
/// `Unchanged`, so a steady feed ships the lamp positions once and the
/// samples thereafter.
#[test]
fn output_frame_probe_if_changed_omits_an_unchanged_layout() {
    let mut harness = Harness::build([0, u16::MAX, 0, u16::MAX]);
    harness.tick();

    let entries = harness.read(ControlDisplayLayoutRead::Always);
    let ControlDisplayLayoutProbeResult::Layout(layout) = &entries[0].display_layout else {
        panic!("expected a layout on the first read");
    };
    let known_revision = layout.revision();

    harness.tick();
    let entries = harness.read(ControlDisplayLayoutRead::IfChanged {
        known_revision: Some(known_revision),
    });
    assert_eq!(
        entries[0].display_layout,
        ControlDisplayLayoutProbeResult::Unchanged {
            revision: known_revision
        },
    );
    assert_eq!(
        entries[0].bytes,
        harness.published_bytes(),
        "samples still ride along when the geometry is gated out"
    );

    // `None` is the cheapest gate of all: no layout work at all.
    let entries = harness.read(ControlDisplayLayoutRead::None);
    assert_eq!(
        entries[0].display_layout,
        ControlDisplayLayoutProbeResult::Omitted
    );
}

/// A shader → fixture → output chain with a counted render path.
struct Harness {
    engine: Engine,
    registry: ProjectRegistry,
    out_id: NodeId,
    sink: RuntimeBufferId,
    renders: Arc<AtomicU32>,
}

impl Harness {
    fn build(color: [u16; 4]) -> Self {
        let mut engine = Engine::new(TreePath::parse("/show.t").expect("root path"));
        let registry = ProjectRegistry::new();
        engine.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let spine = test_placeholder_spine();
        let renders = Arc::new(AtomicU32::new(0));

        let sh_id = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("sh").expect("shader name"),
                lpc_model::NodeName::parse("shader").expect("shader type"),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                spine.clone(),
                frame,
            )
            .expect("add shader");
        engine
            .attach_runtime_node(
                sh_id,
                Box::new(CountingSolidProducer {
                    state: ShaderState::new(VisualProduct::new(sh_id, 0)),
                    renders: Arc::clone(&renders),
                    color,
                }),
                frame,
            )
            .expect("attach shader");

        let fix_id = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("fx").expect("fixture name"),
                lpc_model::NodeName::parse("fixture").expect("fixture type"),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                spine.clone(),
                frame,
            )
            .expect("add fixture");
        engine
            .attach_runtime_node(
                fix_id,
                Box::new(FixtureNode::new(
                    fix_id,
                    MappingConfig::path_points_vec(
                        vec![PathSpec::point_list(0, [[0.5, 0.5]])],
                        2.0,
                    ),
                    lpc_model::FixtureSamplingConfig::TextureArea,
                    frame,
                )),
                frame,
            )
            .expect("attach fixture");
        for (slot, value) in [
            (
                "render_size",
                Dim2u {
                    width: 4,
                    height: 4,
                }
                .to_lp_value(),
            ),
            ("color_order", ColorOrder::Rgb.to_lp_value()),
            ("brightness.some", LpValue::U32(255)),
            ("gamma_correction.some", LpValue::Bool(false)),
        ] {
            bind_literal(&mut engine, fix_id, slot, value, frame);
        }
        bind_produced(
            &mut engine,
            fix_id,
            fixture_input_path(),
            sh_id,
            shader_output_path(),
            frame,
        );

        let out_id = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("out").expect("output name"),
                lpc_model::NodeName::parse("output").expect("output type"),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                spine,
                frame,
            )
            .expect("add output");
        engine
            .attach_runtime_node(out_id, Box::new(OutputNode::new()), frame)
            .expect("attach output");
        let sink = engine
            .runtime_output_sink_buffer_id(out_id)
            .expect("output sink buffer");
        bind_produced(
            &mut engine,
            out_id,
            output_input_path(),
            fix_id,
            SlotPath::parse("output").expect("fixture output slot"),
            frame,
        );
        engine.add_demand_root(out_id);

        Self {
            engine,
            registry,
            out_id,
            sink,
            renders,
        }
    }

    fn tick(&mut self) {
        self.engine.tick(&self.registry, 10).expect("tick");
    }

    fn renders(&self) -> u32 {
        self.renders.load(Ordering::Relaxed)
    }

    /// The output's published runtime buffer, read straight out of the store —
    /// the ground truth the probe's bytes are compared against.
    fn published_bytes(&self) -> Vec<u8> {
        self.engine
            .runtime_buffers()
            .get(self.sink)
            .expect("sink buffer")
            .value()
            .bytes
            .clone()
    }

    fn read(&mut self, display_layout: ControlDisplayLayoutRead) -> Vec<OutputFrameEntry> {
        let results = read_probe_results(
            &mut self.engine,
            &self.registry,
            ProjectReadRequest {
                since: None,
                queries: vec![],
                probes: vec![ProjectProbeRequest::OutputFrame(OutputFrameProbeRequest {
                    display_layout,
                })],
            },
        );
        let [ProjectProbeResult::OutputFrame(OutputFrameProbeResult::Frame { outputs })] =
            results.as_slice()
        else {
            panic!("expected exactly one output-frame probe result, got {results:?}");
        };
        outputs.clone()
    }
}

fn bind_literal(engine: &mut Engine, node: NodeId, slot: &str, value: LpValue, frame: Revision) {
    engine
        .add_binding(
            BindingDraft {
                source: BindingSource::Literal(value),
                target: BindingTarget::ConsumedSlot {
                    node,
                    slot: SlotPath::parse(slot).expect("slot path"),
                },
                priority: BindingPriority::new(0),
                kind: Kind::Choice,
                owner: node,
            },
            frame,
        )
        .expect("bind literal");
}

fn bind_produced(
    engine: &mut Engine,
    consumer: NodeId,
    consumer_slot: SlotPath,
    producer: NodeId,
    producer_slot: SlotPath,
    frame: Revision,
) {
    engine
        .add_binding(
            BindingDraft {
                source: BindingSource::ProducedSlot {
                    node: producer,
                    slot: producer_slot,
                },
                target: BindingTarget::ConsumedSlot {
                    node: consumer,
                    slot: consumer_slot,
                },
                priority: BindingPriority::new(0),
                kind: Kind::Color,
                owner: consumer,
            },
            frame,
        )
        .expect("bind produced slot");
}

/// A solid-color visual producer that counts how often it is asked to render.
struct CountingSolidProducer {
    state: ShaderState,
    renders: Arc<AtomicU32>,
    color: [u16; 4],
}

impl NodeRuntime for CountingSolidProducer {
    fn produce(
        &mut self,
        _slot: &SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        self.state
            .output
            .set_with_version(ctx.revision(), VisualProduct::new(ctx.node_id(), 0));
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
        ShaderState::register_runtime_state_shape(registry).map(|_| ())
    }

    fn render_node(&mut self) -> Option<&mut dyn RenderNode> {
        Some(self)
    }
}

impl RenderNode for CountingSolidProducer {
    fn render_texture(
        &mut self,
        _product: VisualProduct,
        request: &RenderTextureRequest,
        _ctx: &mut RenderContext<'_>,
    ) -> Result<TextureRenderProduct, NodeError> {
        self.renders.fetch_add(1, Ordering::Relaxed);
        let mut pixels = Vec::new();
        let px_count = (request.width as usize).saturating_mul(request.height as usize);
        for _ in 0..px_count {
            match request.format {
                lps_shared::TextureStorageFormat::Rgba16Unorm => {
                    for channel in self.color {
                        pixels.extend_from_slice(&channel.to_le_bytes());
                    }
                }
                lps_shared::TextureStorageFormat::Rgb16Unorm => {
                    for channel in [self.color[0], self.color[1], self.color[2]] {
                        pixels.extend_from_slice(&channel.to_le_bytes());
                    }
                }
                lps_shared::TextureStorageFormat::R16Unorm => {
                    pixels.extend_from_slice(&self.color[0].to_le_bytes());
                }
            }
        }
        TextureRenderProduct::new(request.width, request.height, request.format, pixels)
            .map_err(|e| NodeError::msg(alloc::format!("solid texture: {e}")))
    }
}
