extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;
use lp_engine::MemoryOutputProvider;
use lp_model::{AsLpPath, AsLpPathBuf, ClientMessage, ClientRequest};
use lp_server::{LpServer, handlers::handle_client_message};
use lp_shared::ProjectBuilder;
use lp_shared::fs::{LpFs, LpFsMemory};

#[test]
fn test_stop_all_projects() {
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
    let output_provider: Rc<RefCell<dyn lp_shared::output::OutputProvider>> =
        Rc::new(RefCell::new(MemoryOutputProvider::new()));

    // Create server with prepared filesystem
    let mut server = LpServer::new(
        output_provider.clone(),
        base_fs,
        "projects/".as_path(),
        None,
    );

    // Load project
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
            .unwrap()
        }
    };

    // Verify project is loaded
    assert_eq!(server.project_manager().list_loaded_projects().len(), 1);
    assert_eq!(
        server.project_manager().list_loaded_projects()[0].handle,
        project_handle
    );

    // Send StopAllProjects request
    let request = ClientMessage {
        id: 1,
        msg: ClientRequest::StopAllProjects,
    };

    let server_ptr: *mut LpServer = &mut server;
    let response = unsafe {
        let pm = (*server_ptr).project_manager_mut();
        let fs = (*server_ptr).base_fs_mut();
        handle_client_message(pm, fs, &output_provider, None, request, None).unwrap()
    };

    // Verify response is StopAllProjects
    match response.msg {
        lp_model::server::ServerMsgBody::StopAllProjects => {}
        _ => panic!("Expected StopAllProjects response"),
    }

    // Verify all projects are unloaded
    assert_eq!(server.project_manager().list_loaded_projects().len(), 0);
}
