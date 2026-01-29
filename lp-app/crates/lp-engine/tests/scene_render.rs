extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;
use lp_engine::{MemoryOutputProvider, ProjectRuntime};
use lp_engine_client::ClientProjectView;
use lp_shared::ProjectBuilder;
use lp_shared::fs::LpFsMemory;

#[test]
fn test_scene_render() {
    // ---------------------------------------------------------------------------------------------
    // Arrange
    //
    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());

    // Add nodes

    // - Texture
    let texture_path = builder.texture_basic();

    // - Shader
    builder.shader_basic(&texture_path);

    // - Output
    let output_path = builder.output_basic();

    // - Fixture
    builder.fixture_basic(&output_path, &texture_path);

    // Build project
    builder.build();

    // Create output provider
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));

    // Start runtime with a shared filesystem (Rc<RefCell<>> so changes are visible)
    let mut runtime = ProjectRuntime::new(fs.clone(), output_provider.clone()).unwrap();
    runtime.load_nodes().unwrap();
    runtime.init_nodes().unwrap();
    runtime.ensure_all_nodes_initialized().unwrap();

    // Create a client view
    let mut client_view = ClientProjectView::new();

    // Get output handle
    let output_handle = runtime.handle_for_path(output_path.as_path()).unwrap();

    // Watch output for detail changes
    client_view.watch_detail(output_handle);

    // ---------------------------------------------------------------------------------------------
    // Act & Assert

    // Shader: vec4(mod(time, 1.0), 0.0, 0.0, 1.0) -> RGBA bytes [R, G, B, A]
    // Advancing time by 4ms gives an increment of (4/1000 * 255) = 1.02 ≈ 1

    // Frame 1
    runtime.tick(4).unwrap();
    sync_client_view(&runtime, &mut client_view);
    assert_memory_output_red(&output_provider, 0, 1);

    // Frame 2
    runtime.tick(4).unwrap();
    sync_client_view(&runtime, &mut client_view);
    assert_memory_output_red(&output_provider, 0, 2);

    // Frame 3
    runtime.tick(4).unwrap();
    sync_client_view(&runtime, &mut client_view);
    assert_memory_output_red(&output_provider, 0, 3);

    // Verify client view frame_id matches runtime
    assert_eq!(client_view.frame_id, runtime.frame_id);
}

/// Assert that the first output channel in the memory provider has the expected red value
fn assert_memory_output_red(
    provider: &Rc<RefCell<MemoryOutputProvider>>,
    pin: u32,
    expected_r: u8,
) {
    let handle = provider
        .borrow()
        .get_handle_for_pin(pin)
        .expect("Output channel should be open");

    let data = provider
        .borrow()
        .get_data(handle)
        .expect("Output channel should have data");

    assert!(
        data.len() >= 3,
        "Output data should have at least 3 bytes (RGB) for first channel, got {}",
        data.len()
    );

    let r = data[0];
    let g = data[1];
    let b = data[2];

    assert_eq!(
        r, expected_r,
        "Output channel 0 R: expected {expected_r}, got {r}"
    );
    assert_eq!(g, 0, "Output channel 0 G: expected 0, got {g}");
    assert_eq!(b, 0, "Output channel 0 B: expected 0, got {b}");
}

/// Sync the client view with the runtime
fn sync_client_view(runtime: &ProjectRuntime, client_view: &mut ClientProjectView) {
    let response = runtime
        .get_changes(client_view.frame_id, &client_view.detail_specifier(), None)
        .unwrap();
    client_view.apply_changes(&response).unwrap();
}
