extern crate alloc;

use alloc::collections::BTreeMap;
use lpc_model::{
    Affine2d, Affine2dSlot, FrameId, MapSlot, NodeId, OptionSlot, RatioSlot, RelativeNodeRef,
    RelativeNodeRefSlot, ValueSlot,
};
use lpc_view::ProjectView;
use lpc_wire::WireResourceSummary;
use lpc_wire::legacy::LegacyProjectResponse;

#[test]
fn test_client_view_creation() {
    let view = ProjectView::new();
    assert_eq!(view.frame_id, FrameId::default());
    assert!(view.nodes.is_empty());
    assert!(view.legacy_detail_tracking.is_empty());
    assert!(view.slot_watch_roots.is_empty());
}

#[test]
fn test_slot_watch_specifier() {
    let mut view = ProjectView::new();
    let root = lpc_wire::WireNodeSlotRoot {
        node: NodeId::new(1),
        root: lpc_wire::WireSlotRootKind::State,
    };

    view.watch_slot_root(root);
    assert!(view.slot_watch_roots.contains(&root));

    match view.slot_watch_specifier() {
        lpc_wire::WireSlotWatchSpecifier::ByRoots(roots) => assert_eq!(roots, vec![root]),
        _ => panic!("Expected ByRoots"),
    }

    view.unwatch_slot_root(root);
    assert!(matches!(
        view.slot_watch_specifier(),
        lpc_wire::WireSlotWatchSpecifier::None
    ));
}

#[test]
fn test_request_detail() {
    let mut view = ProjectView::new();
    let handle = NodeId::new(1);

    view.watch_legacy_detail(handle);
    assert!(view.legacy_detail_tracking.contains(&handle));

    // Generate specifier
    let spec = view.legacy_detail_specifier();
    match spec {
        lpc_wire::LegacyWireNodeSpecifier::ByHandles(handles) => {
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

    view.watch_legacy_detail(handle);
    assert!(view.legacy_detail_tracking.contains(&handle));

    view.unwatch_legacy_detail(handle);
    assert!(!view.legacy_detail_tracking.contains(&handle));

    // Generate specifier should be None
    let spec = view.legacy_detail_specifier();
    match spec {
        lpc_wire::LegacyWireNodeSpecifier::None => {}
        _ => panic!("Expected None"),
    }
}

#[test]
fn test_sync_with_changes() {
    let mut view = ProjectView::new();

    // Create a mock response with a created node
    let handle = NodeId::new(1);
    let response = LegacyProjectResponse::GetChanges {
        current_frame: FrameId::new(1),
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![lpc_wire::legacy::LegacyNodeChange::Created {
            handle,
            path: lpc_model::LpPathBuf::from("/src/test.texture"),
            kind: lpc_source::node::NodeKind::Texture,
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
    use lpc_wire::legacy::{LegacyNodeChange, LegacyNodeDetail, LegacyNodeState};

    let mut view = ProjectView::new();
    let handle = NodeId::new(1);
    let path = lpc_model::LpPathBuf::from("/src/s.shader");
    let frame = FrameId::new(1);

    let mut details = BTreeMap::new();
    details.insert(
        handle,
        LegacyNodeDetail {
            path: path.clone(),
            config: Box::new(lpc_source::node::shader::ShaderDef::default()),
            state: LegacyNodeState::Shader(ShaderState::new(frame)),
        },
    );

    let response = LegacyProjectResponse::GetChanges {
        current_frame: frame,
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![LegacyNodeChange::StatusChanged {
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
    use lpc_source::node::texture::TextureDef;
    use lpc_wire::legacy::LegacyNodeState;
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
        lpc_source::node::texture::TextureFormat::Rgb8,
    );

    let initial_response = LegacyProjectResponse::GetChanges {
        current_frame: FrameId::new(1),
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![lpc_wire::legacy::LegacyNodeChange::Created {
            handle,
            path: lpc_model::LpPathBuf::from("/src/test.texture"),
            kind: lpc_source::node::NodeKind::Texture,
        }],
        node_details: {
            let mut map = BTreeMap::new();
            map.insert(
                handle,
                lpc_wire::legacy::LegacyNodeDetail {
                    path: lpc_model::LpPathBuf::from("/src/test.texture"),
                    config: Box::new(TextureDef::new(100, 200)),
                    state: LegacyNodeState::Texture(initial_state),
                },
            );
            map
        },
        theoretical_fps: None,
        resource_summaries: Vec::new(),
        runtime_buffer_payloads: Vec::new(),
        render_product_payloads: Vec::new(),
    };

    view.watch_legacy_detail(handle);
    view.apply_changes(&initial_response).unwrap();

    // Verify initial state is stored
    let entry = view.nodes.get(&handle).unwrap();
    match &entry.state {
        Some(LegacyNodeState::Texture(state)) => {
            assert_eq!(state.texture_data.inline_bytes(), &[10, 20, 30, 40][..]);
            assert_eq!(state.width.value(), &100);
            assert_eq!(state.height.value(), &200);
            assert_eq!(
                state.format.value(),
                &lpc_source::node::texture::TextureFormat::Rgb8
            );
        }
        _ => panic!("Expected Texture state"),
    }

    // Partial update: only width and height changed, texture_data and format should be preserved
    let mut partial_state = TextureState::new(FrameId::new(2));
    partial_state.width.set(FrameId::new(2), 150);
    partial_state.height.set(FrameId::new(2), 250);
    // texture_data and format are NOT set (will be defaults)

    let partial_response = LegacyProjectResponse::GetChanges {
        current_frame: FrameId::new(2),
        since_frame: FrameId::new(1),
        node_handles: vec![handle],
        node_changes: vec![lpc_wire::legacy::LegacyNodeChange::StateUpdated {
            handle,
            state_ver: FrameId::new(2),
        }],
        node_details: {
            let mut map = BTreeMap::new();
            map.insert(
                handle,
                lpc_wire::legacy::LegacyNodeDetail {
                    path: lpc_model::LpPathBuf::from("/src/test.texture"),
                    config: Box::new(TextureDef::new(150, 250)),
                    state: LegacyNodeState::Texture(partial_state),
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
        Some(LegacyNodeState::Texture(state)) => {
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
                &lpc_source::node::texture::TextureFormat::Rgb8,
                "format should be preserved"
            );
        }
        _ => panic!("Expected Texture state"),
    }
}

#[test]
fn test_partial_state_merge_output() {
    use alloc::boxed::Box;
    use lpc_source::node::output::OutputDef;
    use lpc_wire::legacy::LegacyNodeState;
    use lpc_wire::legacy::nodes::output::OutputState;

    let mut view = ProjectView::new();
    let handle = NodeId::new(1);

    // Initial sync: full state with channel_data
    let mut initial_state = OutputState::new(FrameId::new(1));
    initial_state
        .channel_data
        .set_inline(FrameId::new(1), vec![100, 200, 255]);

    let initial_response = LegacyProjectResponse::GetChanges {
        current_frame: FrameId::new(1),
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![lpc_wire::legacy::LegacyNodeChange::Created {
            handle,
            path: lpc_model::LpPathBuf::from("/src/test.output"),
            kind: lpc_source::node::NodeKind::Output,
        }],
        node_details: {
            let mut map = BTreeMap::new();
            map.insert(
                handle,
                lpc_wire::legacy::LegacyNodeDetail {
                    path: lpc_model::LpPathBuf::from("/src/test.output"),
                    config: Box::new(OutputDef::new(0)),
                    state: LegacyNodeState::Output(initial_state),
                },
            );
            map
        },
        theoretical_fps: None,
        resource_summaries: Vec::new(),
        runtime_buffer_payloads: Vec::new(),
        render_product_payloads: Vec::new(),
    };

    view.watch_legacy_detail(handle);
    view.apply_changes(&initial_response).unwrap();

    // Verify initial state is stored
    let entry = view.nodes.get(&handle).unwrap();
    match &entry.state {
        Some(LegacyNodeState::Output(state)) => {
            assert_eq!(state.channel_data.inline_bytes(), &[100, 200, 255][..]);
        }
        _ => panic!("Expected Output state"),
    }

    // Partial update: empty state (no fields changed)
    let partial_state = OutputState::new(FrameId::new(2));
    // channel_data is NOT set (will be default empty)

    let partial_response = LegacyProjectResponse::GetChanges {
        current_frame: FrameId::new(2),
        since_frame: FrameId::new(1),
        node_handles: vec![handle],
        node_changes: vec![],
        node_details: {
            let mut map = BTreeMap::new();
            map.insert(
                handle,
                lpc_wire::legacy::LegacyNodeDetail {
                    path: lpc_model::LpPathBuf::from("/src/test.output"),
                    config: Box::new(OutputDef::new(0)),
                    state: LegacyNodeState::Output(partial_state),
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
        Some(LegacyNodeState::Output(state)) => {
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
    use lpc_source::node::texture::TextureDef;
    use lpc_wire::legacy::nodes::texture::TextureState;
    use lpc_wire::legacy::{LegacyNodeChange, LegacyNodeDetail, LegacyNodeState};

    let mut view = ProjectView::new();
    let handle = NodeId::new(1);
    let path = lpc_model::LpPathBuf::from("/src/t.texture");
    let f1 = FrameId::new(1);

    let mut state = TextureState::new(f1);
    state.width.set(f1, 80);
    state.height.set(f1, 60);

    let response = LegacyProjectResponse::GetChanges {
        current_frame: f1,
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![LegacyNodeChange::Created {
            handle,
            path: path.clone(),
            kind: lpc_source::node::NodeKind::Texture,
        }],
        node_details: {
            let mut m = BTreeMap::new();
            m.insert(
                handle,
                LegacyNodeDetail {
                    path,
                    config: Box::new(TextureDef::new(320, 240)),
                    state: LegacyNodeState::Texture(state),
                },
            );
            m
        },
        theoretical_fps: None,
        resource_summaries: Vec::new(),
        runtime_buffer_payloads: Vec::new(),
        render_product_payloads: Vec::new(),
    };

    view.watch_legacy_detail(handle);
    view.apply_changes(&response).unwrap();

    let cfg = view.nodes[&handle]
        .config
        .as_any()
        .downcast_ref::<TextureDef>()
        .expect("texture config");
    assert_eq!(cfg.width(), 320);
    assert_eq!(cfg.height(), 240);
}

#[test]
fn detail_applies_real_output_config() {
    use alloc::boxed::Box;
    use lpc_source::node::output::{OutputDef, OutputDriverOptionsConfig};
    use lpc_wire::legacy::nodes::output::OutputState;
    use lpc_wire::legacy::{LegacyNodeChange, LegacyNodeDetail, LegacyNodeState};

    let mut view = ProjectView::new();
    let handle = NodeId::new(2);
    let path = lpc_model::LpPathBuf::from("/src/out.output");
    let f1 = FrameId::new(1);

    let state = OutputState::new(f1);
    let mut opts = OutputDriverOptionsConfig::default();
    opts.brightness = RatioSlot::new(0.75);

    let response = LegacyProjectResponse::GetChanges {
        current_frame: f1,
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![LegacyNodeChange::Created {
            handle,
            path: path.clone(),
            kind: lpc_source::node::NodeKind::Output,
        }],
        node_details: {
            let mut m = BTreeMap::new();
            m.insert(
                handle,
                LegacyNodeDetail {
                    path,
                    config: Box::new(OutputDef {
                        pin: ValueSlot::new(42),
                        options: OptionSlot::some(opts.clone()),
                    }),
                    state: LegacyNodeState::Output(state),
                },
            );
            m
        },
        theoretical_fps: None,
        resource_summaries: Vec::new(),
        runtime_buffer_payloads: Vec::new(),
        render_product_payloads: Vec::new(),
    };

    view.watch_legacy_detail(handle);
    view.apply_changes(&response).unwrap();

    let cfg = view.nodes[&handle]
        .config
        .as_any()
        .downcast_ref::<OutputDef>()
        .expect("output config");
    assert_eq!(
        cfg,
        &OutputDef {
            pin: ValueSlot::new(42),
            options: OptionSlot::some(opts),
        }
    );
}

#[test]
fn detail_after_config_updated_replaces_stored_config() {
    use alloc::boxed::Box;
    use lpc_source::node::texture::TextureDef;
    use lpc_wire::legacy::nodes::texture::TextureState;
    use lpc_wire::legacy::{LegacyNodeChange, LegacyNodeDetail, LegacyNodeState};

    let mut view = ProjectView::new();
    let handle = NodeId::new(1);
    let path = lpc_model::LpPathBuf::from("/src/t.texture");

    let mut s1 = TextureState::new(FrameId::new(1));
    s1.width.set(FrameId::new(1), 10);
    s1.height.set(FrameId::new(1), 10);

    view.watch_legacy_detail(handle);
    view.apply_changes(&LegacyProjectResponse::GetChanges {
        current_frame: FrameId::new(1),
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![LegacyNodeChange::Created {
            handle,
            path: path.clone(),
            kind: lpc_source::node::NodeKind::Texture,
        }],
        node_details: {
            let mut m = BTreeMap::new();
            m.insert(
                handle,
                LegacyNodeDetail {
                    path: path.clone(),
                    config: Box::new(TextureDef::new(100, 200)),
                    state: LegacyNodeState::Texture(s1),
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

    view.apply_changes(&LegacyProjectResponse::GetChanges {
        current_frame: FrameId::new(2),
        since_frame: FrameId::new(1),
        node_handles: vec![handle],
        node_changes: vec![LegacyNodeChange::ConfigUpdated {
            handle,
            config_ver: FrameId::new(2),
        }],
        node_details: {
            let mut m = BTreeMap::new();
            m.insert(
                handle,
                LegacyNodeDetail {
                    path: path.clone(),
                    config: Box::new(TextureDef::new(640, 480)),
                    state: LegacyNodeState::Texture(s2),
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
        .downcast_ref::<TextureDef>()
        .expect("texture config");
    assert_eq!(cfg.width(), 640);
    assert_eq!(cfg.height(), 480);
    assert_eq!(view.nodes[&handle].config_ver, FrameId::new(2));
}

#[test]
fn detail_only_entry_stores_real_texture_config() {
    use alloc::boxed::Box;
    use lpc_source::node::texture::TextureDef;
    use lpc_wire::WireNodeStatus;
    use lpc_wire::legacy::nodes::texture::TextureState;
    use lpc_wire::legacy::{LegacyNodeChange, LegacyNodeDetail, LegacyNodeState};

    let mut view = ProjectView::new();
    let handle = NodeId::new(5);
    let path = lpc_model::LpPathBuf::from("/src/only.texture");
    let frame = FrameId::new(1);

    let mut state = TextureState::new(frame);
    state.width.set(frame, 1);
    state.height.set(frame, 1);

    let response = LegacyProjectResponse::GetChanges {
        current_frame: frame,
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![LegacyNodeChange::StatusChanged {
            handle,
            status: WireNodeStatus::Ok,
        }],
        node_details: {
            let mut m = BTreeMap::new();
            m.insert(
                handle,
                LegacyNodeDetail {
                    path: path.clone(),
                    config: Box::new(TextureDef::new(128, 96)),
                    state: LegacyNodeState::Texture(state),
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
        .downcast_ref::<TextureDef>()
        .expect("real texture config, not placeholder zeros");
    assert_eq!(cfg.width(), 128);
    assert_eq!(cfg.height(), 96);
    assert!(matches!(entry.status, WireNodeStatus::Ok));
}

#[test]
fn project_watched_detail_entry_has_state_after_sync() {
    use alloc::boxed::Box;
    use alloc::collections::BTreeMap;
    use lpc_source::node::texture::TextureDef;
    use lpc_wire::legacy::nodes::texture::TextureState;
    use lpc_wire::legacy::{LegacyNodeChange, LegacyNodeDetail, LegacyNodeState};

    let mut view = ProjectView::new();
    let handle = NodeId::new(9);
    let path = lpc_model::LpPathBuf::from("/src/w.texture");
    let frame = FrameId::new(1);

    let mut state = TextureState::new(frame);
    state.width.set(frame, 4);
    state.height.set(frame, 4);

    view.watch_legacy_detail(handle);
    view.apply_changes(&LegacyProjectResponse::GetChanges {
        current_frame: frame,
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![LegacyNodeChange::Created {
            handle,
            path: path.clone(),
            kind: lpc_source::node::NodeKind::Texture,
        }],
        node_details: {
            let mut m = BTreeMap::new();
            m.insert(
                handle,
                LegacyNodeDetail {
                    path,
                    config: Box::new(TextureDef::new(4, 4)),
                    state: LegacyNodeState::Texture(state),
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
    assert!(view.legacy_detail_tracking.contains(&handle));
}

#[test]
fn project_view_resolves_output_bytes_from_resource_cache() {
    use alloc::boxed::Box;
    use alloc::collections::BTreeMap;
    use lpc_model::resource::{ResourceRef, RuntimeBufferId};
    use lpc_source::node::output::OutputDef;
    use lpc_wire::legacy::nodes::output::OutputState;
    use lpc_wire::legacy::{LegacyNodeChange, LegacyNodeDetail, LegacyNodeState};
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

    view.watch_legacy_detail(handle);
    view.apply_changes(&LegacyProjectResponse::GetChanges {
        current_frame: frame,
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![LegacyNodeChange::Created {
            handle,
            path: path.clone(),
            kind: lpc_source::node::NodeKind::Output,
        }],
        node_details: {
            let mut m = BTreeMap::new();
            m.insert(
                handle,
                LegacyNodeDetail {
                    path: path.clone(),
                    config: Box::new(OutputDef::new(0)),
                    state: LegacyNodeState::Output(state),
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
                sample_format: WireChannelSampleFormat::U16,
            },
            bytes: vec![0xAB, 0xCD],
        }],
        render_product_payloads: Vec::new(),
    })
    .unwrap();

    assert_eq!(view.get_output_data(handle).unwrap(), vec![0xCD]);
}

#[test]
fn project_view_resolves_texture_bytes_from_render_product_cache() {
    use alloc::boxed::Box;
    use alloc::collections::BTreeMap;
    use lpc_model::resource::{RenderProductId, ResourceRef};
    use lpc_source::node::texture::TextureDef;
    use lpc_wire::legacy::nodes::texture::TextureState;
    use lpc_wire::legacy::{LegacyNodeChange, LegacyNodeDetail, LegacyNodeState};
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

    view.watch_legacy_detail(handle);
    view.apply_changes(&LegacyProjectResponse::GetChanges {
        current_frame: frame,
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![LegacyNodeChange::Created {
            handle,
            path: path.clone(),
            kind: lpc_source::node::NodeKind::Texture,
        }],
        node_details: {
            let mut m = BTreeMap::new();
            m.insert(
                handle,
                LegacyNodeDetail {
                    path: path.clone(),
                    config: Box::new(TextureDef::new(1, 1)),
                    state: LegacyNodeState::Texture(state),
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
    use lpc_source::node::fixture::{ColorOrder, FixtureDef, MappingConfig};
    use lpc_view::project::resource_cache::resolve_legacy_compat_bytes;
    use lpc_wire::legacy::nodes::fixture::FixtureState;
    use lpc_wire::legacy::{LegacyNodeChange, LegacyNodeDetail, LegacyNodeState};
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

    view.watch_legacy_detail(handle);
    view.apply_changes(&LegacyProjectResponse::GetChanges {
        current_frame: frame,
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![LegacyNodeChange::Created {
            handle,
            path: path.clone(),
            kind: lpc_source::node::NodeKind::Fixture,
        }],
        node_details: {
            let mut m = BTreeMap::new();
            m.insert(
                handle,
                LegacyNodeDetail {
                    path: path.clone(),
                    config: Box::new(FixtureDef {
                        output_loc: RelativeNodeRefSlot::new(
                            RelativeNodeRef::parse("..out").unwrap(),
                        ),
                        texture_loc: RelativeNodeRefSlot::new(
                            RelativeNodeRef::parse("..tex").unwrap(),
                        ),
                        mapping: MappingConfig::path_points(MapSlot::default(), 2.0),
                        color_order: ValueSlot::new(ColorOrder::Rgb),
                        transform: Affine2dSlot::new(Affine2d::identity()),
                        brightness: OptionSlot::none(),
                        gamma_correction: OptionSlot::none(),
                    }),
                    state: LegacyNodeState::Fixture(state),
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
    let LegacyNodeState::Fixture(st) = entry.state.as_ref().expect("fixture detail state") else {
        panic!("fixture state");
    };
    assert_eq!(
        resolve_legacy_compat_bytes(&st.lamp_colors, &view.resource_cache).unwrap(),
        vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06]
    );
}
