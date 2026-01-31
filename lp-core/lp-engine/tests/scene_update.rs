extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;
use lp_engine::{MemoryOutputProvider, ProjectRuntime};
use lp_model::AsLpPath;
use lp_shared::ProjectBuilder;
use lp_shared::fs::LpFsMemory;

#[test]
fn test_node_json_modification() {
    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());

    // Add nodes
    let texture_path = builder.texture_basic();
    builder.shader_basic(&texture_path);
    let output_path = builder.output_basic();
    builder.fixture_basic(&output_path, &texture_path);

    // Build project
    builder.build();
    fs.borrow_mut().reset_changes();

    // Create output provider
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));

    // Start runtime with shared filesystem (Rc<RefCell<>> so changes are visible)
    let mut runtime = ProjectRuntime::new(fs.clone(), output_provider.clone()).unwrap();
    runtime.load_nodes().unwrap();
    runtime.init_nodes().unwrap();
    runtime.ensure_all_nodes_initialized().unwrap();

    // Get shader handle
    let shader_handle = runtime
        .handle_for_path("/src/shader-1.shader".as_path())
        .unwrap();

    // Render a frame to get baseline
    runtime.tick(4).unwrap();

    // Modify shader config (change render_order)
    let shader_config_path = "/src/shader-1.shader/node.json";
    let new_config = r#"{
        "glsl_path": "main.glsl",
        "texture_spec": "/src/texture-1.texture",
        "render_order": 10
    }"#;
    fs.borrow_mut()
        .write_file_mut(shader_config_path.as_path(), new_config.as_bytes())
        .unwrap();

    // Get filesystem changes
    let changes = fs.borrow().get_changes();
    runtime.handle_fs_changes(&changes).unwrap();
    fs.borrow_mut().reset_changes();

    // Advance frame to update frame_id
    runtime.tick(4).unwrap();

    // Verify the change was applied by checking the node's config_ver was updated
    // Get the frame ID before the change (2 frames ago)
    let before_frame = lp_model::FrameId::new((runtime.frame_id.as_i64() - 2).max(0));
    let response = runtime
        .get_changes(
            before_frame,
            &lp_model::project::api::ApiNodeSpecifier::ByHandles(vec![shader_handle]),
            None,
        )
        .unwrap();

    match response {
        lp_model::project::api::ProjectResponse::GetChanges { node_changes, .. } => {
            // Should have a ConfigUpdated change
            assert!(
                node_changes.iter().any(|change| matches!(
                    change,
                    lp_model::project::api::NodeChange::ConfigUpdated { .. }
                )),
                "Expected ConfigUpdated change after node.json modification"
            );
        }
    }
}

#[test]
fn test_main_glsl_modification() {
    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());

    // Add nodes
    let texture_path = builder.texture_basic();
    builder.shader_basic(&texture_path);
    let output_path = builder.output_basic();
    builder.fixture_basic(&output_path, &texture_path);

    // Build project
    builder.build();
    fs.borrow_mut().reset_changes();

    // Create output provider
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));

    // Start runtime with shared filesystem (Rc<RefCell<>> so changes are visible)
    let mut runtime = ProjectRuntime::new(fs.clone(), output_provider.clone()).unwrap();
    runtime.load_nodes().unwrap();
    runtime.init_nodes().unwrap();
    runtime.ensure_all_nodes_initialized().unwrap();

    // Render a frame to get baseline output
    runtime.tick(4).unwrap();
    let baseline_data = output_provider
        .borrow()
        .get_data(
            output_provider
                .borrow()
                .get_handle_for_pin(0)
                .expect("Output channel should be open"),
        )
        .expect("Output channel should have data")
        .to_vec();

    // Modify shader GLSL (change the color)
    fs.borrow_mut()
        .write_file_mut(
            "/src/shader-1.shader/main.glsl".as_path(),
            r#"
                vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
                    return vec4(0.0, mod(time, 1.0), 0.0, 1.0);  // Green instead of red
                }
            "#
            .as_bytes(),
        )
        .unwrap();

    // Get filesystem changes
    let changes = fs.borrow().get_changes();
    runtime.handle_fs_changes(&changes).unwrap();
    fs.borrow_mut().reset_changes();

    // Render another frame to apply the shader changes
    runtime.tick(4).unwrap();

    // Verify the shader was recompiled and applied
    let new_data = output_provider
        .borrow()
        .get_data(
            output_provider
                .borrow()
                .get_handle_for_pin(0)
                .expect("Output channel should be open"),
        )
        .expect("Output channel should have data");

    // The output should now be green (G channel) instead of red (R channel)
    // Baseline had red, new should have green
    assert_ne!(
        baseline_data[0], new_data[0],
        "Red channel should change after GLSL modification"
    );
    assert_ne!(
        baseline_data[1], new_data[1],
        "Green channel should change after GLSL modification"
    );
}

#[test]
fn test_node_deletion() {
    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());

    // Add nodes
    let texture_path = builder.texture_basic();
    builder.shader_basic(&texture_path);
    let output_path = builder.output_basic();
    builder.fixture_basic(&output_path, &texture_path);

    // Build project
    builder.build();
    fs.borrow_mut().reset_changes();

    // Create output provider
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));

    // Start runtime with shared filesystem (Rc<RefCell<>> so changes are visible)
    let mut runtime = ProjectRuntime::new(fs.clone(), output_provider.clone()).unwrap();
    runtime.load_nodes().unwrap();
    runtime.init_nodes().unwrap();
    runtime.ensure_all_nodes_initialized().unwrap();

    // Get shader handle
    let _shader_handle = runtime
        .handle_for_path("/src/shader-1.shader".as_path())
        .unwrap();

    // Delete node.json
    let shader_config_path = "/src/shader-1.shader/node.json";
    fs.borrow_mut()
        .delete_file_mut(shader_config_path.as_path())
        .unwrap();

    // Get filesystem changes
    let changes = fs.borrow().get_changes();
    runtime.handle_fs_changes(&changes).unwrap();
    fs.borrow_mut().reset_changes();

    // Verify the node was removed
    assert!(
        runtime
            .handle_for_path("/src/shader-1.shader".as_path())
            .is_err(),
        "Node should be removed after node.json deletion"
    );
}
