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
    ControlProductGeometry, ControlProductProbeRequest, ControlProductProbeResult,
    GeometryDisplayLayout, KnownRevision, OutputFrameEntry, OutputFrameGeometry,
    OutputFrameProbeRequest, OutputFrameProbeResult, ProjectProbeRequest, ProjectProbeResult,
    ProjectReadRequest, RevisionGateRead, RevisionGateResult, WireChannelSampleFormat,
    WireChildKind, WireSlotIndex,
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
    let entries = harness.read(RevisionGateRead::Always);
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
    assert_eq!(entry.sample_format, Some(WireChannelSampleFormat::U16));

    let geometry = changed(entry);
    assert_eq!(geometry.sample_layout.spans.len(), 1);
    assert_eq!(geometry.sample_layout.spans[0].len, 3);
    assert_eq!(geometry.placements.len(), 1, "one fixture on the wire");

    let GeometryDisplayLayout::Layout(ControlDisplayLayout::Layout2d(layout)) =
        &geometry.display_layout
    else {
        panic!(
            "expected a 2D display layout, got {:?}",
            geometry.display_layout
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
    let first = harness.read(RevisionGateRead::None)[0].revision;

    harness.tick();
    let second = harness.read(RevisionGateRead::None)[0].revision;

    assert!(
        second > first,
        "republished frame must advance the revision ({first:?} -> {second:?})"
    );

    // And a read that does not tick in between sees the same revision — the
    // read itself must not look like a new frame.
    let repeat = harness.read(RevisionGateRead::None)[0].revision;
    assert_eq!(repeat, second, "a re-read is not a new frame");
}

/// Geometry gating: `IfChanged` with the geometry's own revision answers
/// `Unchanged` — no sample layout, no display layout, no placements — so a
/// steady feed ships the whole bundle once and the samples thereafter.
#[test]
fn output_frame_probe_if_changed_omits_unchanged_geometry() {
    let mut harness = Harness::build([0, u16::MAX, 0, u16::MAX]);
    harness.tick();
    let known = harness.geometry_revision();

    harness.tick();
    let entries = harness.read(harness.known(known));
    assert_eq!(
        entries[0].geometry,
        RevisionGateResult::Unchanged { revision: known },
        "a steady read carries no geometry"
    );
    assert_eq!(
        entries[0].bytes,
        harness.published_bytes(),
        "samples still ride along when the geometry is gated out"
    );

    // `None` is the cheapest gate of all: no geometry work at all.
    let entries = harness.read(RevisionGateRead::None);
    assert_eq!(entries[0].geometry, RevisionGateResult::Omitted);
}

/// The gate is per OUTPUT: a revision listed for another node says nothing
/// about this one, which still gets its geometry.
#[test]
fn output_frame_probe_gates_each_output_by_its_own_known_revision() {
    let mut harness = Harness::build([0, u16::MAX, 0, u16::MAX]);
    harness.tick();
    let known = harness.geometry_revision();

    let entries = harness.read(RevisionGateRead::IfChanged {
        known: vec![KnownRevision {
            node: Some(NodeId::new(harness.out_id.0 + 100)),
            revision: known,
        }],
    });
    assert_eq!(changed(&entries[0]).revision, known);
}

/// Trigger 1 — the SAMPLE layout moves alone. A color-order change regroups
/// the samples (`RgbPixels::color_order`) without touching the mapping, so
/// neither the display layout's revision nor the placements move. The
/// geometry revision must, or a client would decode the new frame with the
/// old channel order.
#[test]
fn a_sample_layout_change_moves_the_geometry_revision() {
    let mut harness = Harness::build([0, u16::MAX, 0, u16::MAX]);
    harness.tick();
    let known = harness.geometry_revision();

    let layout_revision_before =
        display_layout_revision(&harness.read(harness.known(Revision::default()))[0]);

    harness.set_fixture_literal("color_order", ColorOrder::Grb.to_lp_value());
    harness.tick();

    let entries = harness.read(harness.known(known));
    let geometry = changed(&entries[0]);
    assert!(
        geometry.revision > known,
        "moved sample layout, moved revision"
    );
    assert_eq!(
        display_layout_revision(&entries[0]),
        layout_revision_before,
        "the display layout did NOT move — the sample layout alone moved the bundle"
    );
    let lpc_model::ControlSampleEncoding::RgbPixels { color_order, .. } =
        &geometry.sample_layout.spans[0].encoding
    else {
        panic!("rgb spans expected");
    };
    assert_eq!(*color_order, ColorOrder::Grb);
}

/// Trigger 2 — the DISPLAY layout moves. A render-size change moves the
/// fixture's display-layout revision (its width/height hints) while the
/// lamp grouping and the wire's cut stay put.
#[test]
fn a_display_layout_change_moves_the_geometry_revision() {
    let mut harness = Harness::build([0, u16::MAX, 0, u16::MAX]);
    harness.tick();
    let known = harness.geometry_revision();

    harness.set_fixture_literal(
        "render_size",
        Dim2u {
            width: 8,
            height: 8,
        }
        .to_lp_value(),
    );
    harness.tick();

    let entries = harness.read(harness.known(known));
    let geometry = changed(&entries[0]);
    assert!(geometry.revision > known);
    let GeometryDisplayLayout::Layout(ControlDisplayLayout::Layout2d(layout)) =
        &geometry.display_layout
    else {
        panic!("expected a layout, got {:?}", geometry.display_layout);
    };
    assert_eq!((layout.width_hint, layout.height_hint), (8, 8));
}

/// A display layout over the link's budget is still ONE answer per
/// revision: it arrives as `Unsupported` inside a changed bundle (sample
/// layout and placements intact), and the next read with that revision is
/// `Unchanged` — the engine does not rebuild and re-measure the refusal
/// every read, and the client does not re-ask for it.
#[test]
fn a_refused_display_layout_is_cached_like_any_other_geometry() {
    let mut harness = Harness::build([0, u16::MAX, 0, u16::MAX]);
    harness.engine.set_display_layout_budget(Some(8));
    harness.tick();

    let entries = harness.read(RevisionGateRead::Always);
    let geometry = changed(&entries[0]);
    assert!(
        matches!(
            geometry.display_layout,
            GeometryDisplayLayout::Unsupported { .. }
        ),
        "an 8-byte budget refuses any layout: {:?}",
        geometry.display_layout
    );
    assert_eq!(geometry.sample_layout.spans.len(), 1, "samples still group");
    let known = geometry.revision;

    harness.tick();
    let entries = harness.read(harness.known(known));
    assert_eq!(
        entries[0].geometry,
        RevisionGateResult::Unchanged { revision: known }
    );
}

/// The control-product probe's gate on the same chain: steady reads are
/// `Unchanged`, and a sample-layout-only change (color order) moves the
/// bundle's revision even though the fixture's display-layout revision stays.
#[test]
fn control_product_geometry_moves_with_its_sample_layout() {
    let mut harness = Harness::build([0, u16::MAX, 0, u16::MAX]);
    harness.tick();

    let first = harness.read_control(RevisionGateRead::Always);
    let RevisionGateResult::Changed(first) = first else {
        panic!("expected changed geometry, got {first:?}");
    };
    let GeometryDisplayLayout::Layout(layout) = &first.display_layout else {
        panic!("expected a layout, got {:?}", first.display_layout);
    };
    let layout_revision = layout.revision();

    harness.tick();
    assert_eq!(
        harness.read_control(RevisionGateRead::if_changed(Some(first.revision))),
        RevisionGateResult::Unchanged {
            revision: first.revision
        },
        "a steady control read carries no geometry"
    );

    harness.set_fixture_literal("color_order", ColorOrder::Grb.to_lp_value());
    harness.tick();
    let RevisionGateResult::Changed(second) =
        harness.read_control(RevisionGateRead::if_changed(Some(first.revision)))
    else {
        panic!("a regrouped sample layout must come back changed");
    };
    assert!(second.revision > first.revision);
    let GeometryDisplayLayout::Layout(layout) = &second.display_layout else {
        panic!("expected a layout");
    };
    assert_eq!(
        layout.revision(),
        layout_revision,
        "the display layout did not move; the sample layout alone did"
    );
}

/// Asked for `U8`, the published `U16` frame travels rounded to nearest —
/// half the bytes, the same post-finalize values — and says so.
#[test]
fn output_frame_probe_rounds_to_u8_when_asked() {
    let mut harness = Harness::build([u16::MAX, 0, 0, u16::MAX]);
    harness.tick();
    assert_eq!(harness.published_bytes(), vec![255, 255, 0, 0, 0, 0]);

    let entries = harness.read_samples(RevisionGateRead::None, Some(WireChannelSampleFormat::U8));

    assert_eq!(entries[0].sample_format, Some(WireChannelSampleFormat::U8));
    assert_eq!(entries[0].bytes, vec![255, 0, 0], "one sample per byte");
    assert_eq!(entries[0].channels, 1, "the lamp count does not change");
}

/// No samples asked: the entry still answers its revision, channel count and
/// geometry — what a module face derives from — with no pixel bytes.
#[test]
fn output_frame_probe_without_samples_still_carries_geometry() {
    let mut harness = Harness::build([u16::MAX, 0, 0, u16::MAX]);
    harness.tick();

    let entries = harness.read_samples(RevisionGateRead::Always, None);

    let entry = &entries[0];
    assert_eq!(entry.sample_format, None);
    assert!(entry.bytes.is_empty());
    assert_eq!(entry.channels, 1);
    assert_eq!(changed(entry).placements.len(), 1);
}

/// The control-product preview renders at 16 bits and ships `U8` when asked:
/// the same samples, rounded.
#[test]
fn control_product_probe_rounds_to_u8_when_asked() {
    let mut harness = Harness::build([0, u16::MAX, 0, u16::MAX]);
    harness.tick();

    let (_, wide_format, wide) =
        harness.read_control_preview(RevisionGateRead::None, WireChannelSampleFormat::U16);
    let (_, narrow_format, narrow) =
        harness.read_control_preview(RevisionGateRead::None, WireChannelSampleFormat::U8);

    assert_eq!(wide_format, WireChannelSampleFormat::U16);
    assert_eq!(narrow_format, WireChannelSampleFormat::U8);
    assert_eq!(narrow.len() * 2, wide.len());
    let rounded: Vec<u8> = wide
        .chunks_exact(2)
        .map(|pair| {
            super::preview_sample_encoding::unorm16_to_unorm8(u16::from_le_bytes([
                pair[0], pair[1],
            ]))
        })
        .collect();
    assert_eq!(narrow, rounded);
}

/// The display layout's OWN revision inside a changed bundle.
fn display_layout_revision(entry: &OutputFrameEntry) -> Revision {
    match &changed(entry).display_layout {
        GeometryDisplayLayout::Layout(layout) => layout.revision(),
        other => panic!("expected a display layout, got {other:?}"),
    }
}

/// The geometry half of an entry, which the test expects to have CHANGED.
fn changed(entry: &OutputFrameEntry) -> &OutputFrameGeometry {
    match &entry.geometry {
        RevisionGateResult::Changed(geometry) => geometry,
        other => panic!("expected changed geometry, got {other:?}"),
    }
}

/// A shader → fixture → output chain with a counted render path.
struct Harness {
    engine: Engine,
    registry: ProjectRegistry,
    sh_id: NodeId,
    fix_id: NodeId,
    out_id: NodeId,
    sink: RuntimeBufferId,
    renders: Arc<AtomicU32>,
    fixture_literals: Vec<(&'static str, LpValue)>,
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
        engine.add_demand_root(out_id);

        let mut harness = Self {
            engine,
            registry,
            sh_id,
            fix_id,
            out_id,
            sink,
            renders,
            fixture_literals: vec![
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
            ],
        };
        harness.bind_all(frame);
        harness
    }

    /// Register the chain's whole binding set: the fixture's literal
    /// settings, shader → fixture, fixture → output.
    fn bind_all(&mut self, frame: Revision) {
        for (slot, value) in self.fixture_literals.clone() {
            bind_literal(&mut self.engine, self.fix_id, slot, value, frame);
        }
        bind_produced(
            &mut self.engine,
            self.fix_id,
            fixture_input_path(),
            self.sh_id,
            shader_output_path(),
            frame,
        );
        bind_produced(
            &mut self.engine,
            self.out_id,
            output_input_path(),
            self.fix_id,
            SlotPath::parse("output").expect("fixture output slot"),
            frame,
        );
    }

    /// Change one of the fixture's literal settings the way a project apply
    /// does: bindings are load-time materializations, so the whole set is
    /// cleared and re-registered rather than a competing binding added.
    fn set_fixture_literal(&mut self, slot: &'static str, value: LpValue) {
        let entry = self
            .fixture_literals
            .iter_mut()
            .find(|(name, _)| *name == slot)
            .expect("a fixture literal the harness binds");
        entry.1 = value;
        let frame = self.engine.revision();
        self.engine.clear_bindings(frame);
        self.bind_all(frame);
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
            .bytes()
            .into_owned()
    }

    /// The output's current geometry revision, read with `Always`.
    fn geometry_revision(&mut self) -> Revision {
        let entries = self.read(RevisionGateRead::Always);
        changed(&entries[0]).revision
    }

    /// A gate claiming this harness's output geometry at `revision`.
    fn known(&self, revision: Revision) -> RevisionGateRead {
        RevisionGateRead::IfChanged {
            known: vec![KnownRevision {
                node: Some(self.out_id),
                revision,
            }],
        }
    }

    /// The fixture's control-product preview geometry, through the real
    /// read stream.
    fn read_control(
        &mut self,
        geometry: RevisionGateRead,
    ) -> RevisionGateResult<ControlProductGeometry> {
        self.read_control_preview(geometry, WireChannelSampleFormat::U16)
            .0
    }

    /// The fixture's control-product preview — geometry, the answered sample
    /// format and its bytes — asked in `sample_format`.
    fn read_control_preview(
        &mut self,
        geometry: RevisionGateRead,
        sample_format: WireChannelSampleFormat,
    ) -> (
        RevisionGateResult<ControlProductGeometry>,
        WireChannelSampleFormat,
        Vec<u8>,
    ) {
        let results = read_probe_results(
            &mut self.engine,
            &self.registry,
            ProjectReadRequest {
                since: None,
                queries: vec![],
                probes: vec![ProjectProbeRequest::ControlProduct(
                    ControlProductProbeRequest {
                        product: lpc_model::ControlProduct::new(
                            self.fix_id,
                            0,
                            lpc_model::ControlExtent::new(1, 3),
                        ),
                        sample_format,
                        geometry,
                    },
                )],
            },
        );
        match results.as_slice() {
            [
                ProjectProbeResult::ControlProduct(ControlProductProbeResult::Preview {
                    geometry,
                    sample_format,
                    bytes,
                    ..
                }),
            ] => (geometry.clone(), *sample_format, bytes.clone()),
            other => panic!("expected one control preview, got {other:?}"),
        }
    }

    /// A full-precision read: samples verbatim at `U16`.
    fn read(&mut self, geometry: RevisionGateRead) -> Vec<OutputFrameEntry> {
        self.read_samples(geometry, Some(WireChannelSampleFormat::U16))
    }

    fn read_samples(
        &mut self,
        geometry: RevisionGateRead,
        samples: Option<WireChannelSampleFormat>,
    ) -> Vec<OutputFrameEntry> {
        let results = read_probe_results(
            &mut self.engine,
            &self.registry,
            ProjectReadRequest {
                since: None,
                queries: vec![],
                probes: vec![ProjectProbeRequest::OutputFrame(OutputFrameProbeRequest {
                    geometry,
                    samples,
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
