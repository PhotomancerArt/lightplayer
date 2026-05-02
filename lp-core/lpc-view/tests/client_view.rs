extern crate alloc;

use alloc::collections::BTreeMap;
use lpc_model::{FrameId, NodeId};
use lpc_view::ProjectView;
use lpc_wire::WireResourceSummary;
use lpc_wire::legacy::ProjectResponse;

#[test]
fn test_client_view_creation() {
    let view = ProjectView::new();
    assert_eq!(view.frame_id, FrameId::default());
    assert!(view.nodes.is_empty());
    assert!(view.detail_tracking.is_empty());
}

#[test]
fn test_request_detail() {
    let mut view = ProjectView::new();
    let handle = NodeId::new(1);

    view.watch_detail(handle);
    assert!(view.detail_tracking.contains(&handle));

    // Generate specifier
    let spec = view.detail_specifier();
    match spec {
        lpc_wire::WireNodeSpecifier::ByHandles(handles) => {
            assert_eq!(handles.len(), 1);
            assert_eq!(handles[0], handle);
        }
        _ => panic!("Expected ByHandles"),
    }
}

#[test]
fn test_stop_detail() {
    let mut view = ProjectView::new();
    let handle = NodeId::new(1);

    view.watch_detail(handle);
    assert!(view.detail_tracking.contains(&handle));

    view.unwatch_detail(handle);
    assert!(!view.detail_tracking.contains(&handle));

    // Generate specifier should be None
    let spec = view.detail_specifier();
    match spec {
        lpc_wire::WireNodeSpecifier::None => {}
        _ => panic!("Expected None"),
    }
}

#[test]
fn test_sync_with_changes() {
    let mut view = ProjectView::new();

    // Create a mock response with a created node
    let handle = NodeId::new(1);
    let response = ProjectResponse::GetChanges {
        current_frame: FrameId::new(1),
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![lpc_wire::legacy::NodeChange::Created {
            handle,
            path: lpc_model::LpPathBuf::from("/src/test.texture"),
            kind: lpc_source::legacy::nodes::NodeKind::Texture,
        }],
        node_details: BTreeMap::new(),
        theoretical_fps: None,
        resource_summaries: Vec::new(),
        runtime_buffer_payloads: Vec::new(),
        render_product_payloads: Vec::new(),
    };

    // Sync
    view.apply_changes(&response).unwrap();

    // Verify view updated
    assert_eq!(view.frame_id, FrameId::new(1));
    assert_eq!(view.nodes.len(), 1);
    assert!(view.nodes.contains_key(&handle));
}

#[test]
fn test_detail_only_entry_uses_pending_status_changed() {
    use alloc::boxed::Box;
    use lpc_wire::WireNodeStatus;
    use lpc_wire::legacy::nodes::shader::ShaderState;
    use lpc_wire::legacy::{NodeChange, NodeDetail, NodeState};

    let mut view = ProjectView::new();
    let handle = NodeId::new(1);
    let path = lpc_model::LpPathBuf::from("/src/s.shader");
    let frame = FrameId::new(1);

    let mut details = BTreeMap::new();
    details.insert(
        handle,
        NodeDetail {
            path: path.clone(),
            config: Box::new(lpc_source::legacy::nodes::shader::ShaderConfig::default()),
            state: NodeState::Shader(ShaderState::new(frame)),
        },
    );

    let response = ProjectResponse::GetChanges {
        current_frame: frame,
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![NodeChange::StatusChanged {
            handle,
            status: WireNodeStatus::Ok,
        }],
        node_details: details,
        theoretical_fps: None,
        resource_summaries: Vec::new(),
        runtime_buffer_payloads: Vec::new(),
        render_product_payloads: Vec::new(),
    };

    view.apply_changes(&response).unwrap();
    let entry = view.nodes.get(&handle).expect("node");
    assert!(matches!(entry.status, WireNodeStatus::Ok));
}

#[test]
fn test_partial_state_merge_texture() {
    use alloc::boxed::Box;
    use lpc_source::legacy::nodes::texture::TextureConfig;
    use lpc_wire::legacy::NodeState;
    use lpc_wire::legacy::nodes::texture::TextureState;

    let mut view = ProjectView::new();
    let handle = NodeId::new(1);

    // Initial sync: full state with texture_data, width, height, format
    let mut initial_state = TextureState::new(FrameId::new(1));
    initial_state
        .texture_data
        .set_inline(FrameId::new(1), vec![10, 20, 30, 40]);
    initial_state.width.set(FrameId::new(1), 100);
    initial_state.height.set(FrameId::new(1), 200);
    initial_state.format.set(
        FrameId::new(1),
        lpc_source::legacy::nodes::texture::TextureFormat::Rgb8,
    );

    let initial_response = ProjectResponse::GetChanges {
        current_frame: FrameId::new(1),
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![lpc_wire::legacy::NodeChange::Created {
            handle,
            path: lpc_model::LpPathBuf::from("/src/test.texture"),
            kind: lpc_source::legacy::nodes::NodeKind::Texture,
        }],
        node_details: {
            let mut map = BTreeMap::new();
            map.insert(
                handle,
                lpc_wire::legacy::NodeDetail {
                    path: lpc_model::LpPathBuf::from("/src/test.texture"),
                    config: Box::new(TextureConfig {
                        width: 100,
                        height: 200,
                    }),
                    state: NodeState::Texture(initial_state),
                },
            );
            map
        },
        theoretical_fps: None,
        resource_summaries: Vec::new(),
        runtime_buffer_payloads: Vec::new(),
        render_product_payloads: Vec::new(),
    };

    view.watch_detail(handle);
    view.apply_changes(&initial_response).unwrap();

    // Verify initial state is stored
    let entry = view.nodes.get(&handle).unwrap();
    match &entry.state {
        Some(NodeState::Texture(state)) => {
            assert_eq!(state.texture_data.inline_bytes(), &[10, 20, 30, 40][..]);
            assert_eq!(state.width.value(), &100);
            assert_eq!(state.height.value(), &200);
            assert_eq!(
                state.format.value(),
                &lpc_source::legacy::nodes::texture::TextureFormat::Rgb8
            );
        }
        _ => panic!("Expected Texture state"),
    }

    // Partial update: only width and height changed, texture_data and format should be preserved
    let mut partial_state = TextureState::new(FrameId::new(2));
    partial_state.width.set(FrameId::new(2), 150);
    partial_state.height.set(FrameId::new(2), 250);
    // texture_data and format are NOT set (will be defaults)

    let partial_response = ProjectResponse::GetChanges {
        current_frame: FrameId::new(2),
        since_frame: FrameId::new(1),
        node_handles: vec![handle],
        node_changes: vec![lpc_wire::legacy::NodeChange::StateUpdated {
            handle,
            state_ver: FrameId::new(2),
        }],
        node_details: {
            let mut map = BTreeMap::new();
            map.insert(
                handle,
                lpc_wire::legacy::NodeDetail {
                    path: lpc_model::LpPathBuf::from("/src/test.texture"),
                    config: Box::new(TextureConfig {
                        width: 150,
                        height: 250,
                    }),
                    state: NodeState::Texture(partial_state),
                },
            );
            map
        },
        theoretical_fps: None,
        resource_summaries: Vec::new(),
        runtime_buffer_payloads: Vec::new(),
        render_product_payloads: Vec::new(),
    };

    view.apply_changes(&partial_response).unwrap();

    // Verify merged state: width/height updated, texture_data and format preserved
    let entry = view.nodes.get(&handle).unwrap();
    match &entry.state {
        Some(NodeState::Texture(state)) => {
            // These should be updated
            assert_eq!(state.width.value(), &150);
            assert_eq!(state.height.value(), &250);
            // These should be preserved from initial state
            assert_eq!(
                state.texture_data.inline_bytes(),
                &[10, 20, 30, 40][..],
                "texture_data should be preserved"
            );
            assert_eq!(
                state.format.value(),
                &lpc_source::legacy::nodes::texture::TextureFormat::Rgb8,
                "format should be preserved"
            );
        }
        _ => panic!("Expected Texture state"),
    }
}

#[test]
fn test_partial_state_merge_output() {
    use alloc::boxed::Box;
    use lpc_source::legacy::nodes::output::OutputConfig;
    use lpc_wire::legacy::NodeState;
    use lpc_wire::legacy::nodes::output::OutputState;

    let mut view = ProjectView::new();
    let handle = NodeId::new(1);

    // Initial sync: full state with channel_data
    let mut initial_state = OutputState::new(FrameId::new(1));
    initial_state
        .channel_data
        .set_inline(FrameId::new(1), vec![100, 200, 255]);

    let initial_response = ProjectResponse::GetChanges {
        current_frame: FrameId::new(1),
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![lpc_wire::legacy::NodeChange::Created {
            handle,
            path: lpc_model::LpPathBuf::from("/src/test.output"),
            kind: lpc_source::legacy::nodes::NodeKind::Output,
        }],
        node_details: {
            let mut map = BTreeMap::new();
            map.insert(
                handle,
                lpc_wire::legacy::NodeDetail {
                    path: lpc_model::LpPathBuf::from("/src/test.output"),
                    config: Box::new(OutputConfig::GpioStrip {
                        pin: 0,
                        options: None,
                    }),
                    state: NodeState::Output(initial_state),
                },
            );
            map
        },
        theoretical_fps: None,
        resource_summaries: Vec::new(),
        runtime_buffer_payloads: Vec::new(),
        render_product_payloads: Vec::new(),
    };

    view.watch_detail(handle);
    view.apply_changes(&initial_response).unwrap();

    // Verify initial state is stored
    let entry = view.nodes.get(&handle).unwrap();
    match &entry.state {
        Some(NodeState::Output(state)) => {
            assert_eq!(state.channel_data.inline_bytes(), &[100, 200, 255][..]);
        }
        _ => panic!("Expected Output state"),
    }

    // Partial update: empty state (no fields changed)
    let partial_state = OutputState::new(FrameId::new(2));
    // channel_data is NOT set (will be default empty)

    let partial_response = ProjectResponse::GetChanges {
        current_frame: FrameId::new(2),
        since_frame: FrameId::new(1),
        node_handles: vec![handle],
        node_changes: vec![],
        node_details: {
            let mut map = BTreeMap::new();
            map.insert(
                handle,
                lpc_wire::legacy::NodeDetail {
                    path: lpc_model::LpPathBuf::from("/src/test.output"),
                    config: Box::new(OutputConfig::GpioStrip {
                        pin: 0,
                        options: None,
                    }),
                    state: NodeState::Output(partial_state),
                },
            );
            map
        },
        theoretical_fps: None,
        resource_summaries: Vec::new(),
        runtime_buffer_payloads: Vec::new(),
        render_product_payloads: Vec::new(),
    };

    view.apply_changes(&partial_response).unwrap();

    // Verify merged state: channel_data should be preserved
    let entry = view.nodes.get(&handle).unwrap();
    match &entry.state {
        Some(NodeState::Output(state)) => {
            assert_eq!(
                state.channel_data.inline_bytes(),
                &[100, 200, 255][..],
                "channel_data should be preserved"
            );
        }
        _ => panic!("Expected Output state"),
    }
}

#[test]
fn detail_applies_real_texture_config() {
    use alloc::boxed::Box;
    use lpc_source::legacy::nodes::texture::TextureConfig;
    use lpc_wire::legacy::nodes::texture::TextureState;
    use lpc_wire::legacy::{NodeChange, NodeDetail, NodeState};

    let mut view = ProjectView::new();
    let handle = NodeId::new(1);
    let path = lpc_model::LpPathBuf::from("/src/t.texture");
    let f1 = FrameId::new(1);

    let mut state = TextureState::new(f1);
    state.width.set(f1, 80);
    state.height.set(f1, 60);

    let response = ProjectResponse::GetChanges {
        current_frame: f1,
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![NodeChange::Created {
            handle,
            path: path.clone(),
            kind: lpc_source::legacy::nodes::NodeKind::Texture,
        }],
        node_details: {
            let mut m = BTreeMap::new();
            m.insert(
                handle,
                NodeDetail {
                    path,
                    config: Box::new(TextureConfig {
                        width: 320,
                        height: 240,
                    }),
                    state: NodeState::Texture(state),
                },
            );
            m
        },
        theoretical_fps: None,
        resource_summaries: Vec::new(),
        runtime_buffer_payloads: Vec::new(),
        render_product_payloads: Vec::new(),
    };

    view.watch_detail(handle);
    view.apply_changes(&response).unwrap();

    let cfg = view.nodes[&handle]
        .config
        .as_any()
        .downcast_ref::<TextureConfig>()
        .expect("texture config");
    assert_eq!(cfg.width, 320);
    assert_eq!(cfg.height, 240);
}

#[test]
fn detail_applies_real_output_config() {
    use alloc::boxed::Box;
    use lpc_source::legacy::nodes::output::{OutputConfig, OutputDriverOptionsConfig};
    use lpc_wire::legacy::nodes::output::OutputState;
    use lpc_wire::legacy::{NodeChange, NodeDetail, NodeState};

    let mut view = ProjectView::new();
    let handle = NodeId::new(2);
    let path = lpc_model::LpPathBuf::from("/src/out.output");
    let f1 = FrameId::new(1);

    let state = OutputState::new(f1);
    let opts = OutputDriverOptionsConfig {
        brightness: 0.75,
        ..OutputDriverOptionsConfig::default()
    };

    let response = ProjectResponse::GetChanges {
        current_frame: f1,
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![NodeChange::Created {
            handle,
            path: path.clone(),
            kind: lpc_source::legacy::nodes::NodeKind::Output,
        }],
        node_details: {
            let mut m = BTreeMap::new();
            m.insert(
                handle,
                NodeDetail {
                    path,
                    config: Box::new(OutputConfig::GpioStrip {
                        pin: 42,
                        options: Some(opts.clone()),
                    }),
                    state: NodeState::Output(state),
                },
            );
            m
        },
        theoretical_fps: None,
        resource_summaries: Vec::new(),
        runtime_buffer_payloads: Vec::new(),
        render_product_payloads: Vec::new(),
    };

    view.watch_detail(handle);
    view.apply_changes(&response).unwrap();

    let cfg = view.nodes[&handle]
        .config
        .as_any()
        .downcast_ref::<OutputConfig>()
        .expect("output config");
    assert_eq!(
        cfg,
        &OutputConfig::GpioStrip {
            pin: 42,
            options: Some(opts),
        }
    );
}

#[test]
fn detail_after_config_updated_replaces_stored_config() {
    use alloc::boxed::Box;
    use lpc_source::legacy::nodes::texture::TextureConfig;
    use lpc_wire::legacy::nodes::texture::TextureState;
    use lpc_wire::legacy::{NodeChange, NodeDetail, NodeState};

    let mut view = ProjectView::new();
    let handle = NodeId::new(1);
    let path = lpc_model::LpPathBuf::from("/src/t.texture");

    let mut s1 = TextureState::new(FrameId::new(1));
    s1.width.set(FrameId::new(1), 10);
    s1.height.set(FrameId::new(1), 10);

    view.watch_detail(handle);
    view.apply_changes(&ProjectResponse::GetChanges {
        current_frame: FrameId::new(1),
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![NodeChange::Created {
            handle,
            path: path.clone(),
            kind: lpc_source::legacy::nodes::NodeKind::Texture,
        }],
        node_details: {
            let mut m = BTreeMap::new();
            m.insert(
                handle,
                NodeDetail {
                    path: path.clone(),
                    config: Box::new(TextureConfig {
                        width: 100,
                        height: 200,
                    }),
                    state: NodeState::Texture(s1),
                },
            );
            m
        },
        theoretical_fps: None,
        resource_summaries: Vec::new(),
        runtime_buffer_payloads: Vec::new(),
        render_product_payloads: Vec::new(),
    })
    .unwrap();

    let mut s2 = TextureState::new(FrameId::new(2));
    s2.width.set(FrameId::new(2), 10);
    s2.height.set(FrameId::new(2), 10);

    view.apply_changes(&ProjectResponse::GetChanges {
        current_frame: FrameId::new(2),
        since_frame: FrameId::new(1),
        node_handles: vec![handle],
        node_changes: vec![NodeChange::ConfigUpdated {
            handle,
            config_ver: FrameId::new(2),
        }],
        node_details: {
            let mut m = BTreeMap::new();
            m.insert(
                handle,
                NodeDetail {
                    path: path.clone(),
                    config: Box::new(TextureConfig {
                        width: 640,
                        height: 480,
                    }),
                    state: NodeState::Texture(s2),
                },
            );
            m
        },
        theoretical_fps: None,
        resource_summaries: Vec::new(),
        runtime_buffer_payloads: Vec::new(),
        render_product_payloads: Vec::new(),
    })
    .unwrap();

    let cfg = view.nodes[&handle]
        .config
        .as_any()
        .downcast_ref::<TextureConfig>()
        .expect("texture config");
    assert_eq!(cfg.width, 640);
    assert_eq!(cfg.height, 480);
    assert_eq!(view.nodes[&handle].config_ver, FrameId::new(2));
}

#[test]
fn detail_only_entry_stores_real_texture_config() {
    use alloc::boxed::Box;
    use lpc_source::legacy::nodes::texture::TextureConfig;
    use lpc_wire::WireNodeStatus;
    use lpc_wire::legacy::nodes::texture::TextureState;
    use lpc_wire::legacy::{NodeChange, NodeDetail, NodeState};

    let mut view = ProjectView::new();
    let handle = NodeId::new(5);
    let path = lpc_model::LpPathBuf::from("/src/only.texture");
    let frame = FrameId::new(1);

    let mut state = TextureState::new(frame);
    state.width.set(frame, 1);
    state.height.set(frame, 1);

    let response = ProjectResponse::GetChanges {
        current_frame: frame,
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![NodeChange::StatusChanged {
            handle,
            status: WireNodeStatus::Ok,
        }],
        node_details: {
            let mut m = BTreeMap::new();
            m.insert(
                handle,
                NodeDetail {
                    path: path.clone(),
                    config: Box::new(TextureConfig {
                        width: 128,
                        height: 96,
                    }),
                    state: NodeState::Texture(state),
                },
            );
            m
        },
        theoretical_fps: None,
        resource_summaries: Vec::new(),
        runtime_buffer_payloads: Vec::new(),
        render_product_payloads: Vec::new(),
    };

    view.apply_changes(&response).unwrap();
    let entry = view.nodes.get(&handle).expect("detail-only node");
    let cfg = entry
        .config
        .as_any()
        .downcast_ref::<TextureConfig>()
        .expect("real texture config, not placeholder zeros");
    assert_eq!(cfg.width, 128);
    assert_eq!(cfg.height, 96);
    assert!(matches!(entry.status, WireNodeStatus::Ok));
}

#[test]
fn project_watched_detail_entry_has_state_after_sync() {
    use alloc::boxed::Box;
    use alloc::collections::BTreeMap;
    use lpc_source::legacy::nodes::texture::TextureConfig;
    use lpc_wire::legacy::nodes::texture::TextureState;
    use lpc_wire::legacy::{NodeChange, NodeDetail, NodeState};

    let mut view = ProjectView::new();
    let handle = NodeId::new(9);
    let path = lpc_model::LpPathBuf::from("/src/w.texture");
    let frame = FrameId::new(1);

    let mut state = TextureState::new(frame);
    state.width.set(frame, 4);
    state.height.set(frame, 4);

    view.watch_detail(handle);
    view.apply_changes(&ProjectResponse::GetChanges {
        current_frame: frame,
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![NodeChange::Created {
            handle,
            path: path.clone(),
            kind: lpc_source::legacy::nodes::NodeKind::Texture,
        }],
        node_details: {
            let mut m = BTreeMap::new();
            m.insert(
                handle,
                NodeDetail {
                    path,
                    config: Box::new(TextureConfig {
                        width: 4,
                        height: 4,
                    }),
                    state: NodeState::Texture(state),
                },
            );
            m
        },
        theoretical_fps: None,
        resource_summaries: Vec::new(),
        runtime_buffer_payloads: Vec::new(),
        render_product_payloads: Vec::new(),
    })
    .unwrap();

    let entry = view.nodes.get(&handle).expect("entry");
    assert!(
        entry.state.is_some(),
        "watched detail sync should populate node state"
    );
    assert!(view.detail_tracking.contains(&handle));
}

#[test]
fn project_view_resolves_output_bytes_from_resource_cache() {
    use alloc::boxed::Box;
    use alloc::collections::BTreeMap;
    use lpc_model::resource::{ResourceRef, RuntimeBufferId};
    use lpc_source::legacy::nodes::output::OutputConfig;
    use lpc_wire::legacy::nodes::output::OutputState;
    use lpc_wire::legacy::{NodeChange, NodeDetail, NodeState};
    use lpc_wire::{
        WireChannelSampleFormat, WireResourceAvailability, WireResourceKindSummary,
        WireResourceMetadataSummary, WireRuntimeBufferKind, WireRuntimeBufferMetadataPayload,
        WireRuntimeBufferPayload,
    };

    let mut view = ProjectView::new();
    let handle = NodeId::new(3);
    let path = lpc_model::LpPathBuf::from("/src/o.output");
    let frame = FrameId::new(1);
    let buf_ref = ResourceRef::runtime_buffer(RuntimeBufferId::new(11));

    let mut state = OutputState::new(frame);
    state.channel_data.set_resource(frame, buf_ref);

    view.watch_detail(handle);
    view.apply_changes(&ProjectResponse::GetChanges {
        current_frame: frame,
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![NodeChange::Created {
            handle,
            path: path.clone(),
            kind: lpc_source::legacy::nodes::NodeKind::Output,
        }],
        node_details: {
            let mut m = BTreeMap::new();
            m.insert(
                handle,
                NodeDetail {
                    path: path.clone(),
                    config: Box::new(OutputConfig::GpioStrip {
                        pin: 0,
                        options: None,
                    }),
                    state: NodeState::Output(state),
                },
            );
            m
        },
        theoretical_fps: None,
        resource_summaries: vec![WireResourceSummary {
            resource_ref: buf_ref,
            changed_frame: frame,
            kind: WireResourceKindSummary::RuntimeBuffer(WireRuntimeBufferKind::OutputChannels),
            metadata: WireResourceMetadataSummary::OutputChannels {
                channels: 1,
                sample_format: WireChannelSampleFormat::U8,
            },
            byte_length_hint: Some(2),
            availability: WireResourceAvailability::Available,
        }],
        runtime_buffer_payloads: vec![WireRuntimeBufferPayload {
            resource_ref: buf_ref,
            changed_frame: frame,
            metadata: WireRuntimeBufferMetadataPayload::OutputChannels {
                channels: 1,
                sample_format: WireChannelSampleFormat::U8,
            },
            bytes: vec![0xAB, 0xCD],
        }],
        render_product_payloads: Vec::new(),
    })
    .unwrap();

    assert_eq!(view.get_output_data(handle).unwrap(), vec![0xAB, 0xCD]);
}

#[test]
fn project_view_resolves_texture_bytes_from_render_product_cache() {
    use alloc::boxed::Box;
    use alloc::collections::BTreeMap;
    use lpc_model::resource::{RenderProductId, ResourceRef};
    use lpc_source::legacy::nodes::texture::TextureConfig;
    use lpc_wire::legacy::nodes::texture::TextureState;
    use lpc_wire::legacy::{NodeChange, NodeDetail, NodeState};
    use lpc_wire::{
        WireRenderProductKind, WireRenderProductPayload, WireResourceAvailability,
        WireResourceKindSummary, WireResourceMetadataSummary, WireTextureFormat,
    };

    let mut view = ProjectView::new();
    let handle = NodeId::new(8);
    let path = lpc_model::LpPathBuf::from("/src/t.texture");
    let frame = FrameId::new(1);
    let prod_ref = ResourceRef::render_product(RenderProductId::new(5));

    let mut state = TextureState::new(frame);
    state.texture_data.set_resource(frame, prod_ref);
    state.width.set(frame, 1);
    state.height.set(frame, 1);

    view.watch_detail(handle);
    view.apply_changes(&ProjectResponse::GetChanges {
        current_frame: frame,
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![NodeChange::Created {
            handle,
            path: path.clone(),
            kind: lpc_source::legacy::nodes::NodeKind::Texture,
        }],
        node_details: {
            let mut m = BTreeMap::new();
            m.insert(
                handle,
                NodeDetail {
                    path: path.clone(),
                    config: Box::new(TextureConfig {
                        width: 1,
                        height: 1,
                    }),
                    state: NodeState::Texture(state),
                },
            );
            m
        },
        theoretical_fps: None,
        resource_summaries: vec![WireResourceSummary {
            resource_ref: prod_ref,
            changed_frame: frame,
            kind: WireResourceKindSummary::RenderProduct(WireRenderProductKind::Texture),
            metadata: WireResourceMetadataSummary::Texture {
                width: 1,
                height: 1,
                format: WireTextureFormat::Rgb8,
            },
            byte_length_hint: Some(3),
            availability: WireResourceAvailability::Available,
        }],
        runtime_buffer_payloads: Vec::new(),
        render_product_payloads: vec![WireRenderProductPayload {
            resource_ref: prod_ref,
            changed_frame: frame,
            width: 1,
            height: 1,
            format: WireTextureFormat::Rgb8,
            bytes: vec![1, 2, 3],
        }],
    })
    .unwrap();

    assert_eq!(view.get_texture_data(handle).unwrap(), vec![1, 2, 3]);
}

#[test]
fn project_view_resolves_fixture_lamp_colors_from_cache() {
    use alloc::boxed::Box;
    use alloc::collections::BTreeMap;
    use lpc_model::resource::{ResourceRef, RuntimeBufferId};
    use lpc_source::legacy::nodes::fixture::{ColorOrder, FixtureConfig, MappingConfig};
    use lpc_view::project::resource_cache::resolve_legacy_compat_bytes;
    use lpc_wire::legacy::nodes::fixture::FixtureState;
    use lpc_wire::legacy::{NodeChange, NodeDetail, NodeState};
    use lpc_wire::{
        WireColorLayout, WireResourceAvailability, WireResourceKindSummary,
        WireResourceMetadataSummary, WireResourceSummary, WireRuntimeBufferKind,
        WireRuntimeBufferMetadataPayload, WireRuntimeBufferPayload,
    };

    let mut view = ProjectView::new();
    let handle = NodeId::new(4);
    let path = lpc_model::LpPathBuf::from("/src/fixture.fixture");
    let frame = FrameId::new(1);
    let buf_ref = ResourceRef::runtime_buffer(RuntimeBufferId::new(3));

    let mut state = FixtureState::new(frame);
    state.lamp_colors.set_resource(frame, buf_ref);

    view.watch_detail(handle);
    view.apply_changes(&ProjectResponse::GetChanges {
        current_frame: frame,
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![NodeChange::Created {
            handle,
            path: path.clone(),
            kind: lpc_source::legacy::nodes::NodeKind::Fixture,
        }],
        node_details: {
            let mut m = BTreeMap::new();
            m.insert(
                handle,
                NodeDetail {
                    path: path.clone(),
                    config: Box::new(FixtureConfig {
                        output_spec: lpc_model::NodeSpec::from("/out"),
                        texture_spec: lpc_model::NodeSpec::from("/tex"),
                        mapping: MappingConfig::PathPoints {
                            paths: vec![],
                            sample_diameter: 2.0,
                        },
                        color_order: ColorOrder::Rgb,
                        transform: [[0.0; 4]; 4],
                        brightness: None,
                        gamma_correction: None,
                    }),
                    state: NodeState::Fixture(state),
                },
            );
            m
        },
        theoretical_fps: None,
        resource_summaries: vec![WireResourceSummary {
            resource_ref: buf_ref,
            changed_frame: frame,
            kind: WireResourceKindSummary::RuntimeBuffer(WireRuntimeBufferKind::FixtureColors),
            metadata: WireResourceMetadataSummary::FixtureColors {
                channels: 3,
                layout: WireColorLayout::Rgb8,
            },
            byte_length_hint: Some(6),
            availability: WireResourceAvailability::Available,
        }],
        runtime_buffer_payloads: vec![WireRuntimeBufferPayload {
            resource_ref: buf_ref,
            changed_frame: frame,
            metadata: WireRuntimeBufferMetadataPayload::FixtureColors {
                channels: 3,
                layout: WireColorLayout::Rgb8,
            },
            bytes: vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06],
        }],
        render_product_payloads: Vec::new(),
    })
    .unwrap();

    let entry = view.nodes.get(&handle).expect("fixture entry");
    let NodeState::Fixture(st) = entry.state.as_ref().expect("fixture detail state") else {
        panic!("fixture state");
    };
    assert_eq!(
        resolve_legacy_compat_bytes(&st.lamp_colors, &view.resource_cache).unwrap(),
        vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06]
    );
}
