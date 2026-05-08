//! [`CoreProjectRuntime`] — owns [`crate::engine::Engine`] plus project services.

use alloc::string::String;

use hashbrown::HashMap;

use lpfs::lp_path::{LpPath, LpPathBuf};
use lpc_model::{Revision, NodeId, TreePath};
use lpfs::FsChange;

use crate::engine::{Engine, EngineError};

use super::{RuntimeServices, SourceAuthoringIndex};

/// Project-level owner: core [`Engine`] plus [`RuntimeServices`] and source authoring snapshots.
pub struct CoreProjectRuntime {
    engine: Engine,
    services: RuntimeServices,
    source_authoring: SourceAuthoringIndex,
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
            source_authoring: SourceAuthoringIndex::new(),
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

    pub fn source_authoring(&self) -> &SourceAuthoringIndex {
        &self.source_authoring
    }

    pub(crate) fn source_authoring_mut(&mut self) -> &mut SourceAuthoringIndex {
        &mut self.source_authoring
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
        let source_authoring_ptr = ptr::from_ref(rt.source_authoring());
        assert_eq!(ptr::from_ref(rt.services()), svc_ptr);
        assert_eq!(ptr::from_ref(rt.source_authoring()), source_authoring_ptr);
        let _ = rt.engine_mut();
        assert_eq!(ptr::from_ref(rt.services()), svc_ptr);
        assert_eq!(ptr::from_ref(rt.source_authoring()), source_authoring_ptr);
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
    use crate::node::{DestroyCtx, MemPressureCtx, NodeRuntime, NodeError, PressureLevel, TickContext};
    use crate::nodes::{FixtureNode, TextureNode, shader_texture_output_path};
    use crate::prop::ProducedSlotAccess;
    use crate::render_product::SolidColorProduct;
    use crate::runtime_buffer::RuntimeBuffer;
    use crate::runtime_product::RuntimeProduct as RpEnum;
    use crate::node::test_placeholder_spine;
    use lpc_model::SlotPath;
    use lpc_model::{Revision, Kind, LpValue, TreePath, WithRevision};
    use lpc_shared::output::{
        MemoryOutputProvider, OutputChannelHandle, OutputDriverOptions, OutputFormat,
        OutputProvider,
    };
    use lpc_model::nodes::fixture::{ColorOrder, MappingConfig, PathSpec, RingOrder};
    use lpc_model::nodes::output::OutputDef;
    use lpc_model::nodes::texture::TextureDef;
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

    #[derive(Clone)]
    struct SolidFixtureOutputs {
        path: SlotPath,
        rid: crate::render_product::RenderProductId,
        last_frame: Revision,
    }

    impl ProducedSlotAccess for SolidFixtureOutputs {
        fn get(&self, path: &SlotPath) -> Option<(RpEnum, Revision)> {
            if path == &self.path {
                Some((RpEnum::render(self.rid), self.last_frame))
            } else {
                None
            }
        }

        fn iter_changed_since<'a>(
            &'a self,
            since: Revision,
        ) -> alloc::boxed::Box<dyn Iterator<Item = (SlotPath, RpEnum, Revision)> + 'a> {
            if self.last_frame.as_i64() > since.as_i64() {
                alloc::boxed::Box::new(core::iter::once((
                    self.path.clone(),
                    RpEnum::render(self.rid),
                    self.last_frame,
                )))
            } else {
                alloc::boxed::Box::new(core::iter::empty())
            }
        }

        fn snapshot<'a>(
            &'a self,
        ) -> alloc::boxed::Box<dyn Iterator<Item = (SlotPath, RpEnum, Revision)> + 'a> {
            alloc::boxed::Box::new(core::iter::once((
                self.path.clone(),
                RpEnum::render(self.rid),
                self.last_frame,
            )))
        }
    }

    struct SolidFixtureProducer {
        out: SolidFixtureOutputs,
        ticks: Arc<AtomicU32>,
    }

    impl NodeRuntime for SolidFixtureProducer {
        fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
            self.ticks.fetch_add(1, Ordering::Relaxed);
            self.out.last_frame = ctx.revision();
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
            &self.out
        }
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

        let tex_id = rt
            .engine_mut()
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("tex").unwrap(),
                lpc_model::NodeName::parse("texture").unwrap(),
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
                tex_id,
                Box::new(TextureNode::new(tex_id, TextureDef::new(4, 4))),
                frame,
            )
            .unwrap();

        let rid = rt
            .engine_mut()
            .render_products_mut()
            .insert(Box::new(SolidColorProduct {
                color: [1.0, 0.0, 0.0, 1.0],
            }));

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

        let out_path = shader_texture_output_path();
        rt.engine_mut()
            .attach_runtime_node(
                sh_id,
                Box::new(SolidFixtureProducer {
                    ticks: Arc::clone(&ticks),
                    out: SolidFixtureOutputs {
                        path: out_path.clone(),
                        rid,
                        last_frame: frame,
                    },
                }),
                frame,
            )
            .unwrap();

        let sink = rt.engine_mut().runtime_buffers_mut().insert(WithRevision::new(
            Revision::default(),
            RuntimeBuffer::raw(alloc::vec![0u8; 6]),
        ));

        rt.services_mut()
            .register_output_sink(sink, &OutputDef::new(pin));

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
                spine,
                artifact,
                frame,
            )
            .unwrap();

        rt.engine_mut()
            .attach_runtime_node(
                fix_id,
                Box::new(FixtureNode::new(
                    fix_id,
                    tex_id,
                    sh_id,
                    mapping,
                    frame,
                    sink,
                    ColorOrder::Rgb,
                    255,
                    false,
                )),
                frame,
            )
            .unwrap();

        rt.engine_mut()
            .bindings_mut()
            .register(
                BindingDraft {
                    source: BindingSource::Literal(LpValue::F32(0.0)),
                    target: BindingTarget::ConsumedSlot {
                        node: fix_id,
                        slot: default_demand_input_path(),
                    },
                    priority: BindingPriority::new(0),
                    kind: Kind::Color,
                    owner: fix_id,
                },
                frame,
            )
            .unwrap();

        rt.engine_mut().add_demand_root(fix_id);

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

        let tex_id = rt
            .engine_mut()
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("tex").unwrap(),
                lpc_model::NodeName::parse("texture").unwrap(),
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
                tex_id,
                Box::new(TextureNode::new(tex_id, TextureDef::new(4, 4))),
                frame,
            )
            .unwrap();

        let rid = rt
            .engine_mut()
            .render_products_mut()
            .insert(Box::new(SolidColorProduct {
                color: [1.0, 0.0, 0.0, 1.0],
            }));

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

        let out_path = shader_texture_output_path();
        rt.engine_mut()
            .attach_runtime_node(
                sh_id,
                Box::new(SolidFixtureProducer {
                    ticks: Arc::clone(&ticks),
                    out: SolidFixtureOutputs {
                        path: out_path.clone(),
                        rid,
                        last_frame: frame,
                    },
                }),
                frame,
            )
            .unwrap();

        let sink_written = rt.engine_mut().runtime_buffers_mut().insert(WithRevision::new(
            Revision::default(),
            RuntimeBuffer::raw(alloc::vec![0u8; 6]),
        ));

        let _sink_idle = rt.engine_mut().runtime_buffers_mut().insert(WithRevision::new(
            Revision::default(),
            RuntimeBuffer::raw(alloc::vec![0xffu8; 6]),
        ));

        rt.services_mut()
            .register_output_sink(sink_written, &OutputDef::new(pin_written));

        rt.services_mut()
            .register_output_sink(_sink_idle, &OutputDef::new(pin_idle));

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
                spine,
                artifact,
                frame,
            )
            .unwrap();

        rt.engine_mut()
            .attach_runtime_node(
                fix_id,
                Box::new(FixtureNode::new(
                    fix_id,
                    tex_id,
                    sh_id,
                    mapping,
                    frame,
                    sink_written,
                    ColorOrder::Rgb,
                    255,
                    false,
                )),
                frame,
            )
            .unwrap();

        rt.engine_mut()
            .bindings_mut()
            .register(
                BindingDraft {
                    source: BindingSource::Literal(LpValue::F32(0.0)),
                    target: BindingTarget::ConsumedSlot {
                        node: fix_id,
                        slot: default_demand_input_path(),
                    },
                    priority: BindingPriority::new(0),
                    kind: Kind::Color,
                    owner: fix_id,
                },
                frame,
            )
            .unwrap();

        rt.engine_mut().add_demand_root(fix_id);

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
    fn fixture_push_marks_output_buffer_dirty_same_frame_before_flush() {
        let mem = Rc::new(MemoryOutputProvider::new());
        let path = TreePath::parse("/show.t").expect("path");
        let mut services = RuntimeServices::new(path.clone());
        services.set_output_provider(Some(Box::new(RcMemoryOutput(Rc::clone(&mem)))));
        let mut rt = CoreProjectRuntime::new(path, services);

        let ticks = Arc::new(AtomicU32::new(0));
        let frame = Revision::new(1);
        let root = rt.engine().tree().root();
        let (spine, artifact) = test_placeholder_spine();

        let tex_id = rt
            .engine_mut()
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("tex").unwrap(),
                lpc_model::NodeName::parse("texture").unwrap(),
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
                tex_id,
                Box::new(TextureNode::new(tex_id, TextureDef::new(4, 4))),
                frame,
            )
            .unwrap();

        let rid = rt
            .engine_mut()
            .render_products_mut()
            .insert(Box::new(SolidColorProduct {
                color: [0.0, 1.0, 0.0, 1.0],
            }));

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

        let out_path = shader_texture_output_path();
        rt.engine_mut()
            .attach_runtime_node(
                sh_id,
                Box::new(SolidFixtureProducer {
                    ticks: Arc::clone(&ticks),
                    out: SolidFixtureOutputs {
                        path: out_path.clone(),
                        rid,
                        last_frame: frame,
                    },
                }),
                frame,
            )
            .unwrap();

        let sink = rt.engine_mut().runtime_buffers_mut().insert(WithRevision::new(
            Revision::default(),
            RuntimeBuffer::raw(alloc::vec![0u8; 6]),
        ));

        rt.services_mut()
            .register_output_sink(sink, &OutputDef::new(99));

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
                spine,
                artifact,
                frame,
            )
            .unwrap();

        rt.engine_mut()
            .attach_runtime_node(
                fix_id,
                Box::new(FixtureNode::new(
                    fix_id,
                    tex_id,
                    sh_id,
                    mapping,
                    frame,
                    sink,
                    ColorOrder::Rgb,
                    255,
                    false,
                )),
                frame,
            )
            .unwrap();

        rt.engine_mut()
            .bindings_mut()
            .register(
                BindingDraft {
                    source: BindingSource::Literal(LpValue::F32(0.0)),
                    target: BindingTarget::ConsumedSlot {
                        node: fix_id,
                        slot: default_demand_input_path(),
                    },
                    priority: BindingPriority::new(0),
                    kind: Kind::Color,
                    owner: fix_id,
                },
                frame,
            )
            .unwrap();

        rt.engine_mut().add_demand_root(fix_id);

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
            "fixture write should bump buffer version to current frame before flush runs",
        );

        let handle = mem.get_handle_for_pin(99).expect("opened");
        let got = mem.get_data(handle).expect("data");
        assert_eq!(got[1], 65535);
    }
}
