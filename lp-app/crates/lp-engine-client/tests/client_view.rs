extern crate alloc;

use alloc::collections::BTreeMap;
use lp_engine_client::ClientProjectView;
use lp_model::{FrameId, NodeHandle, project::api::ProjectResponse};

#[test]
fn test_client_view_creation() {
    let view = ClientProjectView::new();
    assert_eq!(view.frame_id, FrameId::default());
    assert!(view.nodes.is_empty());
    assert!(view.detail_tracking.is_empty());
}

#[test]
fn test_request_detail() {
    let mut view = ClientProjectView::new();
    let handle = NodeHandle::new(1);

    view.watch_detail(handle);
    assert!(view.detail_tracking.contains(&handle));

    // Generate specifier
    let spec = view.detail_specifier();
    match spec {
        lp_model::project::api::ApiNodeSpecifier::ByHandles(handles) => {
            assert_eq!(handles.len(), 1);
            assert_eq!(handles[0], handle);
        }
        _ => panic!("Expected ByHandles"),
    }
}

#[test]
fn test_stop_detail() {
    let mut view = ClientProjectView::new();
    let handle = NodeHandle::new(1);

    view.watch_detail(handle);
    assert!(view.detail_tracking.contains(&handle));

    view.unwatch_detail(handle);
    assert!(!view.detail_tracking.contains(&handle));

    // Generate specifier should be None
    let spec = view.detail_specifier();
    match spec {
        lp_model::project::api::ApiNodeSpecifier::None => {}
        _ => panic!("Expected None"),
    }
}

#[test]
fn test_sync_with_changes() {
    let mut view = ClientProjectView::new();

    // Create a mock response with a created node
    let handle = NodeHandle::new(1);
    let response = ProjectResponse::GetChanges {
        current_frame: FrameId::new(1),
        node_handles: vec![handle],
        node_changes: vec![lp_model::project::api::NodeChange::Created {
            handle,
            path: lp_model::LpPathBuf::from("/src/test.texture"),
            kind: lp_model::NodeKind::Texture,
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
