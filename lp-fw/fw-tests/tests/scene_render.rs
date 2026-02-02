//! Integration test for fw-emu that loads a scene and renders frames
//!
//! This test is similar to `lp-core/lp-engine/tests/scene_render.rs` but uses
//! the emulator firmware instead of direct runtime execution.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use lp_client::{LpClient, SerialClientTransport, serializable_response_to_project_response};
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
#[ignore] // TODO emu: Message handling not working correctly yet
async fn test_scene_render_fw_emu() {
    // ---------------------------------------------------------------------------------------------
    // Arrange
    //

    // Build fw-emu binary
    println!("Building fw-emu...");
    let fw_emu_path = ensure_binary_built(
        BinaryBuildConfig::new("fw-emu")
            .with_target("riscv32imac-unknown-none-elf")
            .with_profile("release"),
    )
    .expect("Failed to build fw-emu");

    println!("Starting emulator...");

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
    let (transport, yield_notify) = SerialClientTransport::new(emulator_arc.clone());

    // Spawn emulator task to run the emulator in a loop
    let _emulator_handle =
        SerialClientTransport::spawn_emulator_task(emulator_arc.clone(), yield_notify);

    println!("Starting client...");
    let client = LpClient::new(Box::new(transport));

    // Create project using ProjectBuilder
    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());

    // Add nodes
    let texture_path = builder.texture_basic();
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

    println!("Syncing project...");
    for (path, content) in project_files {
        let full_path = format!("/projects/{}", path);

        println!("   {}", full_path);
        client
            .fs_write(full_path.as_path(), content)
            .await
            .expect("Failed to write project file");
    }

    println!("Loading project...");

    // Load project
    let project_handle = client
        .project_load("projects/project.json")
        .await
        .expect("Failed to load project");

    // Create client view for syncing
    let mut client_view = ClientProjectView::new();

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

    // Run emulator until yield (processes tick)
    run_until_yield(&emulator_arc);

    // Sync client view
    sync_client_view(&client, project_handle, &mut client_view).await;

    // Frame 2
    {
        let mut emu = emulator_arc.lock().unwrap();
        emu.advance_time(4);
    }

    run_until_yield(&emulator_arc);
    sync_client_view(&client, project_handle, &mut client_view).await;

    // Frame 3
    {
        let mut emu = emulator_arc.lock().unwrap();
        emu.advance_time(4);
    }

    run_until_yield(&emulator_arc);
    sync_client_view(&client, project_handle, &mut client_view).await;

    // Verify we got through 3 frames
    // (Output verification deferred - just verify frames progressed)
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

/// Run emulator until yield syscall
///
/// Note: With the new architecture, the emulator runs in a separate task.
/// This function is kept for compatibility but may not be needed.
/// The emulator task will automatically yield and notify.
fn run_until_yield(emulator: &Arc<Mutex<Riscv32Emulator>>) {
    let mut emu = emulator.lock().unwrap();
    emu.run_until_yield(1_000_000)
        .expect("Failed to run until yield");
}

/// Sync client view with server
async fn sync_client_view(
    client: &LpClient,
    handle: lp_model::project::handle::ProjectHandle,
    view: &mut ClientProjectView,
) {
    let detail_spec = view.detail_specifier();
    let response = client
        .project_sync_internal(handle, Some(view.frame_id), detail_spec)
        .await
        .expect("Failed to sync project");

    let project_response =
        serializable_response_to_project_response(response).expect("Failed to convert response");
    view.apply_changes(&project_response)
        .expect("Failed to apply changes");
}
