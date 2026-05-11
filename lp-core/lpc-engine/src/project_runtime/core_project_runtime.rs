//! [`CoreProjectRuntime`] — owns [`crate::engine::Engine`] plus project services.

use alloc::string::String;

use hashbrown::HashMap;

use lpc_model::{NodeId, Revision, TreePath};
use lpfs::FsChange;
use lpfs::lp_path::{LpPath, LpPathBuf};

use crate::engine::{Engine, EngineError};

use super::RuntimeServices;

/// Project-level owner: core [`Engine`] plus [`RuntimeServices`].
pub struct CoreProjectRuntime {
    engine: Engine,
    services: RuntimeServices,
    artifact_nodes: HashMap<String, NodeId>,
}

impl CoreProjectRuntime {
    /// Creates a runtime with an engine rooted at `root_path`.
    ///
    /// Callers should keep [`RuntimeServices::project_root`] aligned with
    /// `root_path` so project identity matches the engine tree root.
    pub fn new(root_path: TreePath, services: RuntimeServices) -> Self {
        Self {
            engine: Engine::new(root_path),
            services,
            artifact_nodes: HashMap::new(),
        }
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    pub fn services(&self) -> &RuntimeServices {
        &self.services
    }

    pub fn services_mut(&mut self) -> &mut RuntimeServices {
        &mut self.services
    }

    pub fn revision(&self) -> Revision {
        self.engine.revision()
    }

    /// Engine [`NodeId`] for a node artifact path, if loaded.
    pub fn artifact_node_id(&self, path: &LpPath) -> Option<NodeId> {
        self.artifact_nodes.get(path.as_str()).copied()
    }

    pub(crate) fn insert_artifact_node(&mut self, path: LpPathBuf, id: NodeId) {
        self.artifact_nodes.insert(String::from(path.as_str()), id);
    }

    pub fn tick(&mut self, delta_ms: u32) -> Result<(), EngineError> {
        lp_perf::emit_begin!(lp_perf::EVENT_FRAME);
        let result = (|| {
            self.engine.tick(delta_ms)?;
            let revision = self.engine.revision();
            let buffers = self.engine.runtime_buffers();
            self.services
                .flush_dirty_output_sinks(revision, buffers)
                .map_err(|e| EngineError::OutputFlush {
                    message: alloc::format!("{e}"),
                })?;
            Ok(())
        })();
        lp_perf::emit_end!(lp_perf::EVENT_FRAME);
        result
    }

    /// Accept filesystem changes on the M4 core server path.
    ///
    /// Source reload is still follow-up work; this hook exists so server version tracking can advance
    /// without keeping the legacy runtime alive as the active project owner.
    pub fn handle_fs_changes(&mut self, _changes: &[FsChange]) -> Result<(), EngineError> {
        Ok(())
    }

    /// Project sync is disabled until M3 canonical project sync is rebuilt.
    pub fn project_sync_disabled(&self) -> EngineError {
        EngineError::ProjectSyncDisabled {
            message: alloc::string::String::from(
                "project sync is disabled until M3 canonical project sync",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use core::ptr;

    use lpc_model::TreePath;

    use super::*;

    #[test]
    fn new_sets_engine_root_path() {
        let path = TreePath::parse("/demo.show").expect("path");
        let services = RuntimeServices::new(path.clone());
        let rt = CoreProjectRuntime::new(path.clone(), services);
        let root = rt.engine().tree().root();
        let entry = rt.engine().tree().get(root).expect("root entry");
        assert_eq!(entry.path, path);
    }

    #[test]
    fn tick_advances_engine_without_panic() {
        let path = TreePath::parse("/demo.show").expect("path");
        let services = RuntimeServices::new(path.clone());
        let mut rt = CoreProjectRuntime::new(path, services);
        assert_eq!(rt.engine().frame_num().raw(), 0);
        assert_eq!(rt.engine().revision().as_i64(), 0);
        rt.tick(7).expect("tick");
        assert_eq!(rt.engine().frame_num().raw(), 1);
        assert!(rt.engine().revision().as_i64() >= 1);
        assert_eq!(rt.engine().frame_time().delta_ms, 7);
    }

    #[test]
    fn accessors_return_stable_references() {
        let path = TreePath::parse("/demo.show").expect("path");
        let services = RuntimeServices::new(path.clone());
        let mut rt = CoreProjectRuntime::new(path, services);
        let svc_ptr = ptr::from_ref(rt.services());
        assert_eq!(ptr::from_ref(rt.services()), svc_ptr);
        let _ = rt.engine_mut();
        assert_eq!(ptr::from_ref(rt.services()), svc_ptr);
    }
}

#[cfg(test)]
mod output_sink_flush_tests {
    use alloc::boxed::Box;
    use alloc::rc::Rc;
    use alloc::sync::Arc;
    use alloc::vec;
    use core::sync::atomic::{AtomicU32, Ordering};

    use crate::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
    use crate::engine::default_demand_input_path;
    use crate::node::test_placeholder_spine;
    use crate::node::{
        DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, RenderContext,
        RenderNode, TickContext,
    };
    use crate::nodes::{
        FixtureNode, OutputNode, fixture_input_path, output_input_path, shader_output_path,
    };
    use crate::runtime_buffer::RuntimeBufferId;
    use crate::visual_product::{RenderTextureRequest, TextureRenderProduct, VisualProduct};
    use lpc_model::nodes::fixture::{ColorOrder, MappingConfig, PathSpec, RingOrder};
    use lpc_model::nodes::output::OutputDef;
    use lpc_model::{
        Dim2u, Kind, LpValue, Revision, ShaderState, SlotAccess, SlotPath, SlotShapeRegistry,
        SlotShapeRegistryError, StaticSlotShape, ToLpValue, TreePath,
    };
    use lpc_shared::output::{
        MemoryOutputProvider, OutputChannelHandle, OutputDriverOptions, OutputFormat,
        OutputProvider,
    };
    use lpc_wire::{WireChildKind, WireSlotIndex};

    use super::{CoreProjectRuntime, RuntimeServices};

    #[derive(Clone)]
    struct RcMemoryOutput(Rc<MemoryOutputProvider>);

    impl OutputProvider for RcMemoryOutput {
        fn open(
            &self,
            pin: u32,
            byte_count: u32,
            format: OutputFormat,
            options: Option<OutputDriverOptions>,
        ) -> Result<OutputChannelHandle, lpc_shared::error::OutputError> {
            self.0.open(pin, byte_count, format, options)
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

    fn bind_fixture_def_defaults(
        rt: &mut CoreProjectRuntime,
        fix_id: lpc_model::NodeId,
        frame: Revision,
    ) {
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
        rt: &mut CoreProjectRuntime,
        fix_id: lpc_model::NodeId,
        frame: Revision,
        slot: &str,
        value: LpValue,
    ) {
        rt.engine_mut()
            .add_binding(
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
        rt: &mut CoreProjectRuntime,
        root: lpc_model::NodeId,
        spine: lpc_model::NodeInvocation,
        artifact: crate::artifact::ArtifactId,
        frame: Revision,
        name: &str,
        pin: u32,
    ) -> (lpc_model::NodeId, RuntimeBufferId) {
        let out_id = rt
            .engine_mut()
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

        rt.engine_mut()
            .attach_runtime_node(out_id, Box::new(OutputNode::new()), frame)
            .unwrap();
        let sink = rt
            .engine()
            .runtime_output_sink_buffer_id(out_id)
            .expect("output sink buffer");
        rt.services_mut()
            .register_output_sink(sink, &OutputDef::new(pin));
        rt.engine_mut()
            .add_binding(
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
        rt.engine_mut().add_demand_root(out_id);
        (out_id, sink)
    }

    fn attach_idle_output_sink(
        rt: &mut CoreProjectRuntime,
        root: lpc_model::NodeId,
        spine: lpc_model::NodeInvocation,
        artifact: crate::artifact::ArtifactId,
        frame: Revision,
        name: &str,
        pin: u32,
    ) -> (lpc_model::NodeId, RuntimeBufferId) {
        let out_id = rt
            .engine_mut()
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

        rt.engine_mut()
            .attach_runtime_node(out_id, Box::new(OutputNode::new()), frame)
            .unwrap();
        let sink = rt
            .engine()
            .runtime_output_sink_buffer_id(out_id)
            .expect("output sink buffer");
        rt.services_mut()
            .register_output_sink(sink, &OutputDef::new(pin));
        (out_id, sink)
    }

    fn bind_output_to_fixture(
        rt: &mut CoreProjectRuntime,
        out_id: lpc_model::NodeId,
        fix_id: lpc_model::NodeId,
        frame: Revision,
    ) {
        rt.engine_mut()
            .add_binding(
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
    fn project_runtime_output_sink_flush_writes_expected_rgb_via_memory_provider() {
        let mem = Rc::new(MemoryOutputProvider::new());
        let pin = 42u32;

        let path = TreePath::parse("/show.t").expect("path");
        let mut services = RuntimeServices::new(path.clone());
        services.set_output_provider(Some(Box::new(RcMemoryOutput(Rc::clone(&mem)))));
        let mut rt = CoreProjectRuntime::new(path, services);

        let ticks = Arc::new(AtomicU32::new(0));
        let frame = Revision::new(1);
        let root = rt.engine().tree().root();
        let (spine, artifact) = test_placeholder_spine();

        let sh_id = rt
            .engine_mut()
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
        rt.engine_mut()
            .attach_runtime_node(
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
            .engine_mut()
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

        rt.engine_mut()
            .attach_runtime_node(
                fix_id,
                Box::new(FixtureNode::new(fix_id, mapping, frame)),
                frame,
            )
            .unwrap();
        bind_fixture_def_defaults(&mut rt, fix_id, frame);
        rt.engine_mut()
            .add_binding(
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

        let (out_id, _sink) =
            attach_output_demand_root(&mut rt, root, spine.clone(), artifact, frame, "out", pin);
        bind_output_to_fixture(&mut rt, out_id, fix_id, frame);

        rt.tick(10).expect("tick");

        let handle = mem.get_handle_for_pin(pin).expect("channel opened");
        let got = mem.get_data(handle).expect("written");
        assert_eq!(got.len(), 3);
        assert_eq!(got[0], 65535);
        assert_eq!(got[1], 0);
        assert_eq!(got[2], 0);
    }

    #[test]
    fn project_runtime_output_idle_registered_sink_skips_flush_second_pin() {
        let mem = Rc::new(MemoryOutputProvider::new());
        let pin_written = 40u32;
        let pin_idle = 41u32;

        let path = TreePath::parse("/show.t").expect("path");
        let mut services = RuntimeServices::new(path.clone());
        services.set_output_provider(Some(Box::new(RcMemoryOutput(Rc::clone(&mem)))));
        let mut rt = CoreProjectRuntime::new(path, services);

        let ticks = Arc::new(AtomicU32::new(0));
        let frame = Revision::new(1);
        let root = rt.engine().tree().root();
        let (spine, artifact) = test_placeholder_spine();

        let sh_id = rt
            .engine_mut()
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
        rt.engine_mut()
            .attach_runtime_node(
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
            .engine_mut()
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

        rt.engine_mut()
            .attach_runtime_node(
                fix_id,
                Box::new(FixtureNode::new(fix_id, mapping, frame)),
                frame,
            )
            .unwrap();
        bind_fixture_def_defaults(&mut rt, fix_id, frame);
        rt.engine_mut()
            .add_binding(
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
            pin_written,
        );
        bind_output_to_fixture(&mut rt, out_id, fix_id, frame);
        let (_idle_out_id, _sink_idle) = attach_idle_output_sink(
            &mut rt,
            root,
            spine.clone(),
            artifact,
            frame,
            "out_idle",
            pin_idle,
        );

        rt.tick(10).expect("tick");

        assert!(
            mem.is_pin_open(pin_written),
            "written sink should open its pin",
        );
        assert!(
            !mem.is_pin_open(pin_idle),
            "idle sink buffer never updated this frame — should not flush or open",
        );
    }

    #[test]
    fn output_demand_marks_output_buffer_dirty_same_frame_before_flush() {
        let mem = Rc::new(MemoryOutputProvider::new());
        let path = TreePath::parse("/show.t").expect("path");
        let mut services = RuntimeServices::new(path.clone());
        services.set_output_provider(Some(Box::new(RcMemoryOutput(Rc::clone(&mem)))));
        let mut rt = CoreProjectRuntime::new(path, services);

        let ticks = Arc::new(AtomicU32::new(0));
        let frame = Revision::new(1);
        let root = rt.engine().tree().root();
        let (spine, artifact) = test_placeholder_spine();

        let sh_id = rt
            .engine_mut()
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
        rt.engine_mut()
            .attach_runtime_node(
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
            .engine_mut()
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

        rt.engine_mut()
            .attach_runtime_node(
                fix_id,
                Box::new(FixtureNode::new(fix_id, mapping, frame)),
                frame,
            )
            .unwrap();
        bind_fixture_def_defaults(&mut rt, fix_id, frame);
        rt.engine_mut()
            .add_binding(
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

        let (out_id, sink) =
            attach_output_demand_root(&mut rt, root, spine.clone(), artifact, frame, "out", 99);
        bind_output_to_fixture(&mut rt, out_id, fix_id, frame);

        rt.tick(10).expect("tick");

        let ver_frame = rt
            .engine()
            .runtime_buffers()
            .get(sink)
            .expect("sink")
            .changed_at();
        assert_eq!(
            ver_frame.as_i64(),
            rt.engine().revision().as_i64(),
            "output demand should bump buffer version to current frame before flush runs",
        );

        let handle = mem.get_handle_for_pin(99).expect("opened");
        let got = mem.get_data(handle).expect("data");
        assert_eq!(got[1], 65535);
    }
}
