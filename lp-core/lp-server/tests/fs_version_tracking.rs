extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;
use lp_engine::MemoryOutputProvider;
use lp_model::{AsLpPath, AsLpPathBuf};
use lp_server::LpServer;
use lp_shared::fs::LpFsMemory;

#[test]
fn test_fs_changes_not_repeated() {
    // Create server with memory filesystem
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let base_fs = Box::new(LpFsMemory::new());
    let mut server = LpServer::new(output_provider.clone(), base_fs, "projects".as_path(), None);

    // Create a project
    let project_name = "test-project";
    let project_path = "/projects".as_path_buf().join(project_name);

    // Create project.json
    server
        .base_fs_mut()
        .write_file(
            project_path.join("project.json").as_path(),
            b"{\"name\":\"test\",\"uid\":\"test\"}",
        )
        .unwrap();

    // Load the project
    // We need both project_manager_mut() and base_fs_mut(), but can't borrow server mutably twice.
    // Use unsafe to get both references simultaneously via raw pointers.
    let handle = {
        let server_ptr: *mut LpServer = &mut server;
        unsafe {
            let pm = (*server_ptr).project_manager_mut();
            let fs = (*server_ptr).base_fs_mut();
            pm.load_project(&project_path, fs, output_provider.clone())
                .expect("Failed to load project")
        }
    };

    // Get initial version
    let project = server.project_manager().get_project(handle).unwrap();
    let initial_version = project.last_fs_version();
    assert_eq!(initial_version.as_i64(), 0);

    // Write a file to the project
    let file_path = project_path.join("src/test.glsl");
    server
        .base_fs_mut()
        .write_file(file_path.as_path(), b"test content")
        .unwrap();

    // Get the current version after the write
    let current_version_after_write = server.base_fs().current_version();

    // First tick - should process the change
    let responses = server.tick(16, vec![]).unwrap();
    assert_eq!(responses.len(), 0);

    let project = server.project_manager().get_project(handle).unwrap();
    let version_after_first = project.last_fs_version();
    assert!(version_after_first.as_i64() > initial_version.as_i64());
    // After processing, last_fs_version should be current_version.next() (one more than the next version)
    // This ensures that get_changes_since(version_after_first) returns nothing
    assert_eq!(
        version_after_first.as_i64(),
        current_version_after_write.next().as_i64()
    );

    // Second tick - should NOT process the same change again
    // Query changes directly to verify
    let changes_after_first = server.base_fs().get_changes_since(version_after_first);
    assert_eq!(
        changes_after_first.len(),
        0,
        "No changes should be returned after processing"
    );

    let responses = server.tick(16, vec![]).unwrap();
    assert_eq!(responses.len(), 0);

    let project = server.project_manager().get_project(handle).unwrap();
    let version_after_second = project.last_fs_version();
    // Version should not have changed (no new changes to process)
    assert_eq!(version_after_second.as_i64(), version_after_first.as_i64());

    // Write another file
    let file_path2 = project_path.join("src/test2.glsl");
    server
        .base_fs_mut()
        .write_file(file_path2.as_path(), b"test content 2")
        .unwrap();

    // Third tick - should process the new change
    let responses = server.tick(16, vec![]).unwrap();
    assert_eq!(responses.len(), 0);

    let project = server.project_manager().get_project(handle).unwrap();
    let version_after_third = project.last_fs_version();
    // Version should have advanced
    assert!(version_after_third.as_i64() > version_after_first.as_i64());
}
