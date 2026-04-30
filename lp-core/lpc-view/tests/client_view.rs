extern crate alloc;

use alloc::collections::BTreeMap;
use lpc_model::{FrameId, NodeId};
use lpc_view::ProjectView;
use lpl_model::ProjectResponse;

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
        node_changes: vec![lpl_model::NodeChange::Created {
            handle,
            path: lpc_model::LpPathBuf::from("/src/test.texture"),
            kind: lpl_model::NodeKind::Texture,
        }],
        node_details: BTreeMap::new(),
        theoretical_fps: None,
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
    use lpl_model::nodes::shader::ShaderState;
    use lpl_model::{NodeChange, NodeDetail, NodeState};

    let mut view = ProjectView::new();
    let handle = NodeId::new(1);
    let path = lpc_model::LpPathBuf::from("/src/s.shader");
    let frame = FrameId::new(1);

    let mut details = BTreeMap::new();
    details.insert(
        handle,
        NodeDetail {
            path: path.clone(),
            config: Box::new(lpl_model::nodes::shader::ShaderConfig::default()),
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
    };

    view.apply_changes(&response).unwrap();
    let entry = view.nodes.get(&handle).expect("node");
    assert!(matches!(entry.status, WireNodeStatus::Ok));
}

#[test]
fn test_partial_state_merge_texture() {
    use alloc::boxed::Box;
    use lpl_model::NodeState;
    use lpl_model::nodes::texture::{TextureConfig, TextureState};

    let mut view = ProjectView::new();
    let handle = NodeId::new(1);

    // Initial sync: full state with texture_data, width, height, format
    let mut initial_state = TextureState::new(FrameId::new(1));
    initial_state
        .texture_data
        .set(FrameId::new(1), vec![10, 20, 30, 40]);
    initial_state.width.set(FrameId::new(1), 100);
    initial_state.height.set(FrameId::new(1), 200);
    initial_state.format.set(
        FrameId::new(1),
        lpl_model::nodes::texture::TextureFormat::Rgb8,
    );

    let initial_response = ProjectResponse::GetChanges {
        current_frame: FrameId::new(1),
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![lpl_model::NodeChange::Created {
            handle,
            path: lpc_model::LpPathBuf::from("/src/test.texture"),
            kind: lpl_model::NodeKind::Texture,
        }],
        node_details: {
            let mut map = BTreeMap::new();
            map.insert(
                handle,
                lpl_model::NodeDetail {
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
    };

    view.watch_detail(handle);
    view.apply_changes(&initial_response).unwrap();

    // Verify initial state is stored
    let entry = view.nodes.get(&handle).unwrap();
    match &entry.state {
        Some(NodeState::Texture(state)) => {
            assert_eq!(state.texture_data.value(), &vec![10, 20, 30, 40]);
            assert_eq!(state.width.value(), &100);
            assert_eq!(state.height.value(), &200);
            assert_eq!(
                state.format.value(),
                &lpl_model::nodes::texture::TextureFormat::Rgb8
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
        node_changes: vec![lpl_model::NodeChange::StateUpdated {
            handle,
            state_ver: FrameId::new(2),
        }],
        node_details: {
            let mut map = BTreeMap::new();
            map.insert(
                handle,
                lpl_model::NodeDetail {
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
                state.texture_data.value(),
                &vec![10, 20, 30, 40],
                "texture_data should be preserved"
            );
            assert_eq!(
                state.format.value(),
                &lpl_model::nodes::texture::TextureFormat::Rgb8,
                "format should be preserved"
            );
        }
        _ => panic!("Expected Texture state"),
    }
}

#[test]
fn test_partial_state_merge_output() {
    use alloc::boxed::Box;
    use lpl_model::NodeState;
    use lpl_model::nodes::output::{OutputConfig, OutputState};

    let mut view = ProjectView::new();
    let handle = NodeId::new(1);

    // Initial sync: full state with channel_data
    let mut initial_state = OutputState::new(FrameId::new(1));
    initial_state
        .channel_data
        .set(FrameId::new(1), vec![100, 200, 255]);

    let initial_response = ProjectResponse::GetChanges {
        current_frame: FrameId::new(1),
        since_frame: FrameId::default(),
        node_handles: vec![handle],
        node_changes: vec![lpl_model::NodeChange::Created {
            handle,
            path: lpc_model::LpPathBuf::from("/src/test.output"),
            kind: lpl_model::NodeKind::Output,
        }],
        node_details: {
            let mut map = BTreeMap::new();
            map.insert(
                handle,
                lpl_model::NodeDetail {
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
    };

    view.watch_detail(handle);
    view.apply_changes(&initial_response).unwrap();

    // Verify initial state is stored
    let entry = view.nodes.get(&handle).unwrap();
    match &entry.state {
        Some(NodeState::Output(state)) => {
            assert_eq!(state.channel_data.value(), &vec![100, 200, 255]);
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
                lpl_model::NodeDetail {
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
    };

    view.apply_changes(&partial_response).unwrap();

    // Verify merged state: channel_data should be preserved
    let entry = view.nodes.get(&handle).unwrap();
    match &entry.state {
        Some(NodeState::Output(state)) => {
            assert_eq!(
                state.channel_data.value(),
                &vec![100, 200, 255],
                "channel_data should be preserved"
            );
        }
        _ => panic!("Expected Output state"),
    }
}
