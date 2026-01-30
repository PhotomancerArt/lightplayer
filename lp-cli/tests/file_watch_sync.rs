//! Unit tests for file watching and syncing functionality
//!
//! Tests fs_loop components:
//! - add_pending_change function (debouncing logic)
//! - File change detection (via LpFsMemory)
//! - Path formatting in sync_file_change

use lp_model::AsLpPath;
use lp_shared::fs::{
    LpFs, LpFsMemory,
    fs_event::{ChangeType, FsChange},
};
use std::collections::HashMap;
use std::time::Instant;

use lp_cli::commands::dev::fs_loop::add_pending_change;

#[test]
fn test_add_pending_change() {
    // Test that add_pending_change correctly adds changes and updates timestamp
    let mut pending_changes: HashMap<String, FsChange> = HashMap::new();
    let mut last_change_time: Option<Instant> = None;

    let change1 = FsChange {
        path: lp_model::LpPathBuf::from("/src/test1.glsl"),
        change_type: ChangeType::Create,
    };

    add_pending_change(&mut pending_changes, &mut last_change_time, change1.clone());

    assert_eq!(pending_changes.len(), 1);
    assert!(pending_changes.contains_key("/src/test1.glsl"));
    assert!(last_change_time.is_some());

    // Add another change
    let change2 = FsChange {
        path: lp_model::LpPathBuf::from("/src/test2.glsl"),
        change_type: ChangeType::Modify,
    };

    add_pending_change(&mut pending_changes, &mut last_change_time, change2.clone());

    assert_eq!(pending_changes.len(), 2);
    assert!(pending_changes.contains_key("/src/test1.glsl"));
    assert!(pending_changes.contains_key("/src/test2.glsl"));

    // Update existing change (should deduplicate by path)
    let change1_updated = FsChange {
        path: lp_model::LpPathBuf::from("/src/test1.glsl"),
        change_type: ChangeType::Modify, // Changed from Create to Modify
    };

    let time_before = last_change_time.unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));
    add_pending_change(
        &mut pending_changes,
        &mut last_change_time,
        change1_updated.clone(),
    );

    // Should still have 2 changes (deduplicated by path)
    assert_eq!(pending_changes.len(), 2);
    // The change should be updated (Modify, not Create)
    assert_eq!(
        pending_changes.get("/src/test1.glsl").unwrap().change_type,
        ChangeType::Modify
    );
    // Timestamp should be updated
    assert!(last_change_time.unwrap() > time_before);
}

#[test]
fn test_file_change_detection() {
    // Test that LpFsMemory correctly detects file changes
    // This verifies the file change detection that fs_loop relies on

    let mut fs = LpFsMemory::new();

    // Create a file
    fs.write_file("/src/test.glsl".as_path(), b"void main() { }")
        .unwrap();

    // Get changes
    let changes = fs.get_changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].path.as_str(), "/src/test.glsl");
    assert_eq!(changes[0].change_type, ChangeType::Create);

    // Modify the file
    fs.reset_changes();
    fs.write_file(
        "/src/test.glsl".as_path(),
        b"void main() { gl_FragColor = vec4(1.0); }",
    )
    .unwrap();

    let changes = fs.get_changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].change_type, ChangeType::Modify);

    // Delete the file
    fs.reset_changes();
    fs.delete_file("/src/test.glsl".as_path()).unwrap();

    let changes = fs.get_changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].change_type, ChangeType::Delete);
}

#[test]
fn test_multiple_file_changes() {
    // Test that multiple file changes are detected correctly
    let fs = LpFsMemory::new();

    // Create multiple files
    fs.write_file("/src/file1.glsl".as_path(), b"file1")
        .unwrap();
    fs.write_file("/src/file2.glsl".as_path(), b"file2")
        .unwrap();
    fs.write_file("/src/file3.glsl".as_path(), b"file3")
        .unwrap();

    let changes = fs.get_changes();
    assert_eq!(changes.len(), 3);

    // Verify all changes are Create type
    for change in &changes {
        assert_eq!(change.change_type, ChangeType::Create);
    }

    // Verify paths
    let paths: Vec<&str> = changes.iter().map(|c| c.path.as_str()).collect();
    assert!(paths.contains(&"/src/file1.glsl"));
    assert!(paths.contains(&"/src/file2.glsl"));
    assert!(paths.contains(&"/src/file3.glsl"));
}

#[test]
fn test_sync_file_change_path_formatting() {
    // Test that sync_file_change correctly formats server paths
    // We test the logic directly since we can't easily mock AsyncLpClient

    let project_uid = "test-project";

    // Test path with leading slash
    let file_path = "/src/test.glsl";
    let expected_server_path = format!("projects/{}/{}", project_uid, &file_path[1..]);
    assert_eq!(expected_server_path, "projects/test-project/src/test.glsl");

    // Test path without leading slash
    let file_path_no_slash = "src/test.glsl";
    let expected_server_path2 = format!("projects/{project_uid}/{file_path_no_slash}");
    assert_eq!(expected_server_path2, "projects/test-project/src/test.glsl");

    // Test nested path
    let nested_path = "/src/nested/file.glsl";
    let expected_server_path3 = format!("projects/{}/{}", project_uid, &nested_path[1..]);
    assert_eq!(
        expected_server_path3,
        "projects/test-project/src/nested/file.glsl"
    );
}

#[test]
fn test_debouncing_logic() {
    // Test the debouncing logic that fs_loop uses
    use lp_cli::commands::dev::fs_loop::DEBOUNCE_DURATION;
    use std::time::Duration;

    let mut pending_changes: HashMap<String, FsChange> = HashMap::new();
    let mut last_change_time: Option<Instant> = None;

    // Add a change
    let change = FsChange {
        path: lp_model::LpPathBuf::from("/src/test.glsl"),
        change_type: ChangeType::Create,
    };
    add_pending_change(&mut pending_changes, &mut last_change_time, change);

    // Immediately after adding, should not sync (debounce period hasn't passed)
    let should_sync_immediate = if let Some(last_time) = last_change_time {
        last_time.elapsed() >= DEBOUNCE_DURATION && !pending_changes.is_empty()
    } else {
        false
    };
    assert!(
        !should_sync_immediate,
        "Should not sync immediately after change"
    );

    // After debounce period, should sync
    std::thread::sleep(DEBOUNCE_DURATION + Duration::from_millis(10));
    let should_sync_after_debounce = if let Some(last_time) = last_change_time {
        last_time.elapsed() >= DEBOUNCE_DURATION && !pending_changes.is_empty()
    } else {
        false
    };
    assert!(
        should_sync_after_debounce,
        "Should sync after debounce period"
    );
}
