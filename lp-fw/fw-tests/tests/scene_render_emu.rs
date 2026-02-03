//! Integration test for fw-emu that loads a scene and renders frames
//!
//! This test is similar to `lp-core/lp-engine/tests/scene_render_emu` but uses
//! the emulator firmware instead of direct runtime execution.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use fw_tests::transport_emu_serial::SerialEmuClientTransport;
use log;
use lp_client::{LpClient, serializable_response_to_project_response};
use lp_engine_client::ClientProjectView;
use lp_model::{AsLpPath, FrameId};
use lp_riscv_elf::load_elf;
use lp_riscv_emu::{
    LogLevel, Riscv32Emulator, TimeMode,
    test_util::{BinaryBuildConfig, ensure_binary_built},
};
use lp_riscv_inst::Gpr;
use lp_shared::ProjectBuilder;
use lp_shared::fs::LpFsMemory;

#[tokio::test]
#[test_log::test]
async fn test_scene_render_fw_emu() {
    // ---------------------------------------------------------------------------------------------
    // Arrange
    //
    // Build fw-emu binary
    log::info!("Building fw-emu...");
    let fw_emu_path = ensure_binary_built(
        BinaryBuildConfig::new("fw-emu")
            .with_target("riscv32imac-unknown-none-elf")
            .with_profile("release"),
    )
    .expect("Failed to build fw-emu");

    log::info!("Starting emulator...");

    // Load ELF
    let elf_data = std::fs::read(&fw_emu_path).expect("Failed to read fw-emu ELF");
    let load_info = load_elf(&elf_data).expect("Failed to load ELF");

    // Create emulator with simulated time mode
    let ram_size = load_info.ram.len();
    let mut emulator = Riscv32Emulator::new(load_info.code, load_info.ram)
        .with_log_level(LogLevel::None)
        .with_max_instructions(10_000_000)
        .with_time_mode(TimeMode::Simulated(0));

    // Set up stack pointer
    let sp_value = 0x80000000u32.wrapping_add((ram_size as u32).wrapping_sub(16));
    emulator.set_register(Gpr::Sp, sp_value as i32);

    // Set PC to entry point
    emulator.set_pc(load_info.entry_point);

    // Create shared emulator reference
    let emulator_arc = Arc::new(Mutex::new(emulator));

    // Create serial client transport
    let transport = SerialEmuClientTransport::new(emulator_arc.clone());

    log::info!("Starting client...");
    let client = LpClient::new(Box::new(transport));

    // Create project using ProjectBuilder
    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());

    // Add nodes
    let texture_path = builder.texture().width(2).height(2).add(&mut builder);
    builder.shader_basic(&texture_path);
    let output_path = builder.output_basic();
    builder.fixture_basic(&output_path, &texture_path);
    builder.build();

    // ---------------------------------------------------------------------------------------------
    // Act: Send project files to firmware
    //

    // Write project files to firmware filesystem via client
    // Get all files from the project filesystem
    let project_files = collect_project_files(&fs.borrow());

    log::info!("Syncing project...");
    // Write files to /projects/project/ directory structure
    let project_dir = "project";
    for (path, content) in project_files {
        let full_path = format!("/projects/{}/{}", project_dir, path);

        log::info!("   {}", full_path);
        client
            .fs_write(full_path.as_path(), content)
            .await
            .expect("Failed to write project file");
    }

    log::info!("Loading project...");

    // Load project (pass directory name, not file path)
    let project_handle = client
        .project_load(project_dir)
        .await
        .expect("Failed to load project");

    // Create client view for syncing
    let mut client_view = ClientProjectView::new();

    // Initial sync to get all nodes (they may not be initialized yet)
    sync_client_view(&client, project_handle, &mut client_view).await;

    // Initial sync to get all nodes (using All to populate the view)
    sync_client_view(&client, project_handle, &mut client_view).await;

    // Find output node handle by path
    let output_handle = client_view
        .nodes
        .iter()
        .find(|(_, entry)| entry.path.as_str() == output_path.as_str())
        .map(|(handle, _)| *handle)
        .expect("Output node not found in client view");

    // Watch output for detail changes
    client_view.watch_detail(output_handle);

    // ---------------------------------------------------------------------------------------------
    // Act & Assert: Render frames
    //

    // Shader: vec4(mod(time, 1.0), 0.0, 0.0, 1.0) -> RGBA bytes [R, G, B, A]
    // Advancing time by 4ms gives an increment of (4/1000 * 255) = 1.02 ≈ 1

    // Frame 1
    {
        let mut emu = emulator_arc.lock().unwrap();
        emu.advance_time(4);
    }
    sync_client_view(&client, project_handle, &mut client_view).await;
    assert_output_red(&client_view, output_handle, 1);

    // Frame 2
    {
        let mut emu = emulator_arc.lock().unwrap();
        emu.advance_time(4);
    }
    sync_client_view(&client, project_handle, &mut client_view).await;
    assert_output_red(&client_view, output_handle, 2);

    // Frame 3
    {
        let mut emu = emulator_arc.lock().unwrap();
        emu.advance_time(4);
    }
    sync_client_view(&client, project_handle, &mut client_view).await;
    assert_output_red(&client_view, output_handle, 3);

    // Verify we got through 3 frames
    assert!(
        client_view.frame_id >= FrameId(3),
        "Should have processed at least 3 frames"
    );
}

/// Collect all files from project filesystem
fn collect_project_files(fs: &LpFsMemory) -> Vec<(String, Vec<u8>)> {
    use lp_shared::fs::LpFs;

    // List all files recursively
    let entries = fs
        .list_dir("/".as_path(), true)
        .expect("Failed to list project files");

    let mut files = Vec::new();
    for entry in entries {
        // Skip directories
        if entry.as_str().ends_with('/') {
            continue;
        }

        // Check if it's a directory (list_dir may return dirs without trailing /)
        if fs.is_dir(entry.as_path()).unwrap_or(false) {
            continue;
        }

        // Read file content
        let content = fs
            .read_file(entry.as_path())
            .expect("Failed to read project file");

        // Remove leading / for relative path
        let relative_path = if entry.as_str().starts_with('/') {
            &entry.as_str()[1..]
        } else {
            entry.as_str()
        };

        files.push((relative_path.to_string(), content));
    }

    files
}

/// Sync client view with server
async fn sync_client_view(
    client: &LpClient,
    handle: lp_model::project::handle::ProjectHandle,
    view: &mut ClientProjectView,
) {
    // For initial sync (empty view), request all nodes to populate the list
    // Otherwise use normal detail_specifier
    let is_initial_sync = view.nodes.is_empty();
    let detail_spec = if is_initial_sync {
        lp_model::project::api::ApiNodeSpecifier::All
    } else {
        view.detail_specifier()
    };

    let response = client
        .project_sync_internal(handle, Some(view.frame_id), detail_spec)
        .await
        .expect("Failed to sync project");

    let project_response =
        serializable_response_to_project_response(response).expect("Failed to convert response");
    view.apply_changes(&project_response)
        .expect("Failed to apply changes");
}

/// Assert that the output channel has the expected red value
fn assert_output_red(view: &ClientProjectView, handle: lp_model::NodeHandle, expected_r: u8) {
    let data = view
        .get_output_data(handle)
        .expect("Failed to get output data");

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
        "Output channel 0 R: expected {expected_r}, got {r} (data: {:?})",
        data
    );
    assert_eq!(g, 0, "Output channel 0 G: expected 0, got {g}");
    assert_eq!(b, 0, "Output channel 0 B: expected 0, got {b}");
}
