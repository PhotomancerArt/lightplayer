extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;
use lp_engine::MemoryOutputProvider;
use lp_engine_client::ClientProjectView;
use lp_model::{AsLpPath, AsLpPathBuf};
use lp_server::LpServer;
use lp_shared::ProjectBuilder;
use lp_shared::fs::{LpFs, LpFsMemory};

#[test]
fn test_server_tick_propagates_to_projects() {
    // Create project using ProjectBuilder in a temporary filesystem
    let temp_fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(temp_fs.clone());

    // Add nodes
    let texture_path = builder.texture_basic();
    builder.shader_basic(&texture_path);
    let output_path = builder.output_basic();
    builder.fixture_basic(&output_path, &texture_path);

    // Build project (creates files at root of temp_fs)
    builder.build();

    // Copy project files to server filesystem under projects/test-project/
    let project_name = "test-project";
    let project_prefix = "/projects".as_path_buf().join(project_name);

    // Prepare base filesystem with project files
    let base_fs = Box::new(LpFsMemory::new());

    // Copy project.json
    let project_json = temp_fs
        .borrow()
        .read_file("/project.json".as_path())
        .unwrap();
    base_fs
        .write_file(project_prefix.join("project.json").as_path(), &project_json)
        .unwrap();

    // Copy all node files
    let node_paths = vec![
        texture_path.to_path_buf(),
        "/src/shader-0.shader".as_path_buf(),
        output_path.to_path_buf(),
        "/src/fixture-0.fixture".as_path_buf(),
    ];

    for node_path in &node_paths {
        // Copy node.json
        let node_json_path = node_path.join("node.json");
        if let Ok(data) = temp_fs.borrow().read_file(node_json_path.as_path()) {
            // Strip leading '/' to make it relative for joining
            let relative_path = node_json_path
                .as_str()
                .strip_prefix('/')
                .unwrap_or(node_json_path.as_str());
            base_fs
                .write_file(project_prefix.join(relative_path).as_path(), &data)
                .unwrap();
        }

        // Copy GLSL file if it's a shader
        if node_path.as_str().contains(".shader") {
            let glsl_path = node_path.join("main.glsl");
            if let Ok(data) = temp_fs.borrow().read_file(glsl_path.as_path()) {
                // Strip leading '/' to make it relative for joining
                let relative_path = glsl_path
                    .as_str()
                    .strip_prefix('/')
                    .unwrap_or(glsl_path.as_str());
                base_fs
                    .write_file(project_prefix.join(relative_path).as_path(), &data)
                    .unwrap();
            }
        }
    }

    // Create output provider
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));

    // Create server with prepared filesystem
    let mut server = LpServer::new(output_provider.clone(), base_fs, "projects/".as_path());

    // Load project
    // We need both project_manager_mut() and base_fs_mut(), but can't borrow server mutably twice.
    // Use unsafe to get both references simultaneously via raw pointers.
    // This is safe because the references point to different fields and are used immediately.
    let project_handle = {
        let server_ptr: *mut LpServer = &mut server;
        unsafe {
            let pm = (*server_ptr).project_manager_mut();
            let fs = (*server_ptr).base_fs_mut();
            pm.load_project(
                &"/".as_path_buf().join(project_name),
                fs,
                output_provider.clone(),
            )
            .expect("Failed to load project")
        }
    };

    // Create client view
    let mut client_view = ClientProjectView::new();

    // Get output handle from server project
    let project = server
        .project_manager()
        .get_project(project_handle)
        .expect("Project should be loaded");

    // Debug: print all nodes
    println!("Loaded nodes:");
    for (handle, entry) in &project.runtime().nodes {
        println!(
            "  Handle: {:?}, Kind: {:?}, Path: {}",
            handle,
            entry.kind,
            entry.path.as_str()
        );
    }

    // Find output node - use the path from ProjectBuilder but relative to project root
    // ProjectBuilder creates paths like "/src/output-0.output", but after loading into server
    // they should still be accessible via the same path (project runtime uses chrooted fs)
    let output_handle = project
        .runtime()
        .nodes
        .iter()
        .find(|(_, entry)| entry.kind == lp_model::NodeKind::Output)
        .map(|(handle, _)| *handle)
        .expect("Output node should exist");

    // Watch output for detail changes
    client_view.watch_detail(output_handle);

    // Initial sync
    sync_client_view_from_server(&server, project_handle, &mut client_view);

    // Verify initial frame_id
    let initial_frame_id = client_view.frame_id;
    let project_runtime_frame_id = server
        .project_manager()
        .get_project(project_handle)
        .unwrap()
        .runtime()
        .frame_id;
    assert_eq!(initial_frame_id, project_runtime_frame_id);

    // Frame 1: Tick server
    server.tick(4, vec![]).expect("Server tick should succeed");
    sync_client_view_from_server(&server, project_handle, &mut client_view);

    // Verify frame_id advanced
    let frame_1_id = client_view.frame_id;
    let project_runtime_frame_id_1 = server
        .project_manager()
        .get_project(project_handle)
        .unwrap()
        .runtime()
        .frame_id;
    assert_eq!(frame_1_id, project_runtime_frame_id_1);
    assert!(frame_1_id.as_i64() > initial_frame_id.as_i64());

    // Frame 2: Tick server again
    server.tick(4, vec![]).expect("Server tick should succeed");
    sync_client_view_from_server(&server, project_handle, &mut client_view);

    // Verify frame_id advanced again
    let frame_2_id = client_view.frame_id;
    let project_runtime_frame_id_2 = server
        .project_manager()
        .get_project(project_handle)
        .unwrap()
        .runtime()
        .frame_id;
    assert_eq!(frame_2_id, project_runtime_frame_id_2);
    assert!(frame_2_id.as_i64() > frame_1_id.as_i64());

    // Frame 3: Tick server again
    server.tick(4, vec![]).expect("Server tick should succeed");
    sync_client_view_from_server(&server, project_handle, &mut client_view);

    // Verify frame_id advanced again
    let frame_3_id = client_view.frame_id;
    let project_runtime_frame_id_3 = server
        .project_manager()
        .get_project(project_handle)
        .unwrap()
        .runtime()
        .frame_id;
    assert_eq!(frame_3_id, project_runtime_frame_id_3);
    assert!(frame_3_id.as_i64() > frame_2_id.as_i64());

    // Final verification: client view frame_id matches project runtime
    assert_eq!(client_view.frame_id, project_runtime_frame_id_3);
}

/// Sync the client view with the server project
fn sync_client_view_from_server(
    server: &LpServer,
    project_handle: lp_model::project::handle::ProjectHandle,
    client_view: &mut ClientProjectView,
) {
    let project = server
        .project_manager()
        .get_project(project_handle)
        .expect("Project should be loaded");

    let response = project
        .runtime()
        .get_changes(client_view.frame_id, &client_view.detail_specifier(), None)
        .expect("get_changes should succeed");
    client_view
        .apply_changes(&response)
        .expect("apply_changes should succeed");
}
