//! [`CoreProjectRuntime`] — owns [`crate::engine::Engine`] plus project services.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use hashbrown::HashMap;

use lpc_model::lp_path::{LpPath, LpPathBuf};
use lpc_model::{FrameId, NodeId, TreePath};
use lpfs::FsChange;

use lpc_wire::legacy::{NodeChange, ProjectResponse};
use lpc_wire::{
    RenderProductPayloadRequest, ResourceSummarySpecifier, RuntimeBufferPayloadSpecifier,
    WireNodeSpecifier, WireNodeStatus,
};

use crate::engine::{Engine, EngineError};

use super::{
    CompatibilityProjection, RuntimeServices,
    detail_projection::build_node_detail_map,
    kind::legacy_node_kind_from_tree_path,
    resource_projection::{
        buffer_payload_interest, render_payload_interest, render_product_payloads_for_request,
        runtime_buffer_payloads_for_request, summarize_render_products_if_requested,
        summarize_runtime_buffers_if_requested,
    },
};

/// Project-level owner: core [`Engine`] plus [`RuntimeServices`] and compatibility
/// projection for the M4 stack.
pub struct CoreProjectRuntime {
    engine: Engine,
    services: RuntimeServices,
    compatibility: CompatibilityProjection,
    legacy_src_dirs: HashMap<String, NodeId>,
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
            compatibility: CompatibilityProjection::new(),
            legacy_src_dirs: HashMap::new(),
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

    pub fn compatibility(&self) -> &CompatibilityProjection {
        &self.compatibility
    }

    pub(crate) fn compatibility_mut(&mut self) -> &mut CompatibilityProjection {
        &mut self.compatibility
    }

    pub fn frame_id(&self) -> FrameId {
        self.engine.frame_id()
    }

    /// Engine [`NodeId`] for a legacy `/src/...<kind>` directory, if loaded.
    pub fn legacy_src_node_id(&self, dir: &LpPath) -> Option<NodeId> {
        self.legacy_src_dirs.get(dir.as_str()).copied()
    }

    pub(crate) fn insert_legacy_src_dir(&mut self, dir: LpPathBuf, id: NodeId) {
        self.legacy_src_dirs.insert(String::from(dir.as_str()), id);
    }

    pub fn tick(&mut self, delta_ms: u32) -> Result<(), EngineError> {
        self.engine.tick(delta_ms)?;
        let frame_id = self.engine.frame_id();
        let buffers = self.engine.runtime_buffers();
        self.services
            .flush_dirty_output_sinks(frame_id, buffers)
            .map_err(|e| EngineError::OutputFlush {
                message: alloc::format!("{e}"),
            })?;
        Ok(())
    }

    /// Accept filesystem changes on the M4 core server path.
    ///
    /// Source reload is still follow-up work; this hook exists so server version tracking can advance
    /// without keeping the legacy runtime alive as the active project owner.
    pub fn handle_fs_changes(&mut self, _changes: &[FsChange]) -> Result<(), EngineError> {
        Ok(())
    }

    /// M4.1+: projects tree membership, statuses, incremental changes, compatibility node details,
    /// and optionally resource summaries plus buffer/render-product payloads per request specifiers.
    pub fn get_changes(
        &self,
        since_frame: FrameId,
        detail_specifier: &WireNodeSpecifier,
        resource_summary_specifier: ResourceSummarySpecifier,
        runtime_buffer_payload_specifier: &RuntimeBufferPayloadSpecifier,
        render_product_payload_request: &RenderProductPayloadRequest,
        theoretical_fps: Option<f32>,
    ) -> Result<ProjectResponse, EngineError> {
        let mut node_handles = Vec::new();
        let mut node_changes = Vec::new();

        for entry in self.engine.tree().entries() {
            if entry.id == self.engine.tree().root() {
                continue;
            }

            let Some(kind) = legacy_node_kind_from_tree_path(&entry.path) else {
                continue;
            };

            node_handles.push(entry.id);

            if entry.created_frame.as_i64() > since_frame.as_i64() {
                node_changes.push(NodeChange::Created {
                    handle: entry.id,
                    path: LpPathBuf::from(entry.path.to_string()),
                    kind,
                });
                node_changes.push(NodeChange::ConfigUpdated {
                    handle: entry.id,
                    config_ver: entry.created_frame,
                });
            }

            if entry.change_frame.as_i64() > since_frame.as_i64() {
                node_changes.push(NodeChange::StateUpdated {
                    handle: entry.id,
                    state_ver: entry.change_frame,
                });
            }

            if entry.change_frame.as_i64() > since_frame.as_i64()
                || since_frame == FrameId::default()
            {
                node_changes.push(NodeChange::StatusChanged {
                    handle: entry.id,
                    status: projected_status(entry.status.clone()),
                });
            }
        }

        let node_details = build_node_detail_map(
            self.engine(),
            self.compatibility(),
            detail_specifier,
            self.frame_id(),
        );

        let mut resource_summaries = alloc::vec::Vec::new();
        summarize_runtime_buffers_if_requested(
            since_frame,
            resource_summary_specifier,
            self.engine.runtime_buffers(),
            &mut resource_summaries,
        );
        summarize_render_products_if_requested(
            since_frame,
            resource_summary_specifier,
            self.engine.render_products(),
            &mut resource_summaries,
        );

        let mut runtime_buffer_payloads = alloc::vec::Vec::new();
        if let Some(ref interest) = buffer_payload_interest(runtime_buffer_payload_specifier) {
            runtime_buffer_payloads_for_request(
                since_frame,
                interest,
                self.engine.runtime_buffers(),
                &mut runtime_buffer_payloads,
            );
        }

        let mut render_product_payloads = alloc::vec::Vec::new();
        if let Some(ref interest) = render_payload_interest(render_product_payload_request) {
            render_product_payloads_for_request(
                since_frame,
                interest,
                self.engine.render_products(),
                &mut render_product_payloads,
            );
        }

        Ok(ProjectResponse::GetChanges {
            current_frame: self.frame_id(),
            since_frame,
            node_handles,
            node_changes,
            node_details,
            theoretical_fps,
            resource_summaries,
            runtime_buffer_payloads,
            render_product_payloads,
        })
    }
}

fn projected_status(status: WireNodeStatus) -> WireNodeStatus {
    match status {
        WireNodeStatus::Created => WireNodeStatus::Ok,
        other => other,
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
        assert_eq!(rt.engine().frame_id().as_i64(), 0);
        rt.tick(7).expect("tick");
        assert_eq!(rt.engine().frame_id().as_i64(), 1);
        assert_eq!(rt.engine().frame_time().delta_ms, 7);
    }

    #[test]
    fn accessors_return_stable_references() {
        let path = TreePath::parse("/demo.show").expect("path");
        let services = RuntimeServices::new(path.clone());
        let mut rt = CoreProjectRuntime::new(path, services);
        let svc_ptr = ptr::from_ref(rt.services());
        let compat_ptr = ptr::from_ref(rt.compatibility());
        assert_eq!(ptr::from_ref(rt.services()), svc_ptr);
        assert_eq!(ptr::from_ref(rt.compatibility()), compat_ptr);
        let _ = rt.engine_mut();
        assert_eq!(ptr::from_ref(rt.services()), svc_ptr);
        assert_eq!(ptr::from_ref(rt.compatibility()), compat_ptr);
    }
}

#[cfg(test)]
mod output_sink_flush_tests {
    use alloc::boxed::Box;
    use alloc::rc::Rc;
    use alloc::sync::Arc;
    use alloc::vec;
    use core::sync::atomic::{AtomicU32, Ordering};

    use lpc_model::prop::PropPath;
    use lpc_model::{FrameId, Kind, ModelValue, TreePath, Versioned};
    use lpc_shared::output::{
        MemoryOutputProvider, OutputChannelHandle, OutputDriverOptions, OutputFormat,
        OutputProvider,
    };
    use lpc_source::SrcValueSpec;
    use lpc_source::legacy::nodes::fixture::{ColorOrder, MappingConfig, PathSpec, RingOrder};
    use lpc_source::legacy::nodes::output::OutputConfig;
    use lpc_source::legacy::nodes::texture::TextureConfig;
    use lpc_wire::{WireChildKind, WireSlotIndex};
    use lps_shared::LpsValueF32;

    use crate::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
    use crate::engine::default_demand_input_path;
    use crate::node::{DestroyCtx, MemPressureCtx, Node, NodeError, PressureLevel, TickContext};
    use crate::nodes::{FixtureNode, TextureNode, shader_texture_output_path};
    use crate::prop::{RuntimeOutputAccess, RuntimePropAccess};
    use crate::render_product::SolidColorProduct;
    use crate::runtime_buffer::RuntimeBuffer;
    use crate::runtime_product::RuntimeProduct as RpEnum;
    use crate::tree::test_placeholder_spine;

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
        path: PropPath,
        rid: crate::render_product::RenderProductId,
        last_frame: FrameId,
    }

    impl RuntimeOutputAccess for SolidFixtureOutputs {
        fn get(&self, path: &PropPath) -> Option<(RpEnum, FrameId)> {
            if path == &self.path {
                Some((RpEnum::render(self.rid), self.last_frame))
            } else {
                None
            }
        }
    }

    struct SolidFixtureProducer {
        out: SolidFixtureOutputs,
        ticks: Arc<AtomicU32>,
    }

    impl Node for SolidFixtureProducer {
        fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
            self.ticks.fetch_add(1, Ordering::Relaxed);
            self.out.last_frame = ctx.frame_id();
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

        fn props(&self) -> &dyn RuntimePropAccess {
            struct Empty;
            impl RuntimePropAccess for Empty {
                fn get(&self, _path: &PropPath) -> Option<(LpsValueF32, FrameId)> {
                    None
                }
                fn iter_changed_since<'b>(
                    &'b self,
                    _since: FrameId,
                ) -> alloc::boxed::Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'b>
                {
                    alloc::boxed::Box::new(core::iter::empty())
                }
                fn snapshot<'b>(
                    &'b self,
                ) -> alloc::boxed::Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'b>
                {
                    alloc::boxed::Box::new(core::iter::empty())
                }
            }
            static EMPTY: Empty = Empty;
            &EMPTY
        }

        fn outputs(&self) -> &dyn RuntimeOutputAccess {
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
        let frame = FrameId::new(1);
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
                Box::new(TextureNode::new(
                    tex_id,
                    TextureConfig {
                        width: 4,
                        height: 4,
                    },
                )),
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

        let sink = rt.engine_mut().runtime_buffers_mut().insert(Versioned::new(
            FrameId::default(),
            RuntimeBuffer::raw(alloc::vec![0u8; 6]),
        ));

        rt.services_mut()
            .register_output_sink(sink, &OutputConfig::GpioStrip { pin, options: None });

        let mapping = MappingConfig::PathPoints {
            paths: vec![PathSpec::RingArray {
                center: (0.5, 0.5),
                diameter: 1.0,
                start_ring_inclusive: 0,
                end_ring_exclusive: 1,
                ring_lamp_counts: vec![1],
                offset_angle: 0.0,
                order: RingOrder::InnerFirst,
            }],
            sample_diameter: 2.0,
        };

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
                    source: BindingSource::Literal(SrcValueSpec::Literal(ModelValue::F32(0.0))),
                    target: BindingTarget::NodeInput {
                        node: fix_id,
                        input: default_demand_input_path(),
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
        let frame = FrameId::new(1);
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
                Box::new(TextureNode::new(
                    tex_id,
                    TextureConfig {
                        width: 4,
                        height: 4,
                    },
                )),
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

        let sink_written = rt.engine_mut().runtime_buffers_mut().insert(Versioned::new(
            FrameId::default(),
            RuntimeBuffer::raw(alloc::vec![0u8; 6]),
        ));

        let _sink_idle = rt.engine_mut().runtime_buffers_mut().insert(Versioned::new(
            FrameId::default(),
            RuntimeBuffer::raw(alloc::vec![0xffu8; 6]),
        ));

        rt.services_mut().register_output_sink(
            sink_written,
            &OutputConfig::GpioStrip {
                pin: pin_written,
                options: None,
            },
        );

        rt.services_mut().register_output_sink(
            _sink_idle,
            &OutputConfig::GpioStrip {
                pin: pin_idle,
                options: None,
            },
        );

        let mapping = MappingConfig::PathPoints {
            paths: vec![PathSpec::RingArray {
                center: (0.5, 0.5),
                diameter: 1.0,
                start_ring_inclusive: 0,
                end_ring_exclusive: 1,
                ring_lamp_counts: vec![1],
                offset_angle: 0.0,
                order: RingOrder::InnerFirst,
            }],
            sample_diameter: 2.0,
        };

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
                    source: BindingSource::Literal(SrcValueSpec::Literal(ModelValue::F32(0.0))),
                    target: BindingTarget::NodeInput {
                        node: fix_id,
                        input: default_demand_input_path(),
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
        let frame = FrameId::new(1);
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
                Box::new(TextureNode::new(
                    tex_id,
                    TextureConfig {
                        width: 4,
                        height: 4,
                    },
                )),
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

        let sink = rt.engine_mut().runtime_buffers_mut().insert(Versioned::new(
            FrameId::default(),
            RuntimeBuffer::raw(alloc::vec![0u8; 6]),
        ));

        rt.services_mut().register_output_sink(
            sink,
            &OutputConfig::GpioStrip {
                pin: 99,
                options: None,
            },
        );

        let mapping = MappingConfig::PathPoints {
            paths: vec![PathSpec::RingArray {
                center: (0.5, 0.5),
                diameter: 1.0,
                start_ring_inclusive: 0,
                end_ring_exclusive: 1,
                ring_lamp_counts: vec![1],
                offset_angle: 0.0,
                order: RingOrder::InnerFirst,
            }],
            sample_diameter: 2.0,
        };

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
                    source: BindingSource::Literal(SrcValueSpec::Literal(ModelValue::F32(0.0))),
                    target: BindingTarget::NodeInput {
                        node: fix_id,
                        input: default_demand_input_path(),
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
            .changed_frame();
        assert_eq!(
            ver_frame.as_i64(),
            rt.engine().frame_id().as_i64(),
            "fixture write should bump buffer version to current frame before flush runs",
        );

        let handle = mem.get_handle_for_pin(99).expect("opened");
        let got = mem.get_data(handle).expect("data");
        assert_eq!(got[1], 65535);
    }
}
