# Phase 6: Create Integration Test

## Scope of phase

Create the integration test that loads a scene and renders frames using the emulator firmware.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Create integration test (`lp-app/apps/fw-emu/tests/scene_render.rs`)

```rust
//! Integration test for fw-emu that loads a scene and renders frames
//!
//! This test is similar to `lp-core/lp-engine/tests/scene_render.rs` but uses
//! the emulator firmware instead of direct runtime execution.

extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;

use lp_client::{LpClient, SerialClientTransport};
use lp_engine_client::ClientProjectView;
use lp_model::project::api::ApiNodeSpecifier;
use lp_riscv_elf::load_elf;
use lp_riscv_emu::{LogLevel, Riscv32Emulator, TimeMode, test_util::BinaryBuildConfig};
use lp_riscv_inst::Gpr;
use lp_shared::fs::LpFsMemory;
use lp_shared::ProjectBuilder;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn test_scene_render_fw_emu() {
    // ---------------------------------------------------------------------------------------------
    // Arrange
    //

    // Build fw-emu binary
    let fw_emu_path = lp_riscv_emu::test_util::ensure_binary_built(
        BinaryBuildConfig::new("fw-emu")
    ).expect("Failed to build fw-emu");

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
    let transport = SerialClientTransport::new(emulator_arc.clone());
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

    for (path, content) in project_files {
        let full_path = format!("projects/{}", path);
        client.fs_write(full_path.as_str(), &content).await
            .expect("Failed to write project file");
    }

    // Load project
    let project_handle = client.project_load("projects/project.json").await
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
    assert!(client_view.frame_id >= 3, "Should have processed at least 3 frames");
}

/// Collect all files from project filesystem
fn collect_project_files(fs: &LpFsMemory) -> Vec<(String, Vec<u8>)> {
    // TODO: Implement file collection from LpFsMemory
    // For now, return empty - we'll need to implement this based on LpFsMemory API
    vec![]
}

/// Run emulator until yield syscall
fn run_until_yield(emulator: &Arc<Mutex<Riscv32Emulator>>) {
    let mut emu = emulator.lock().unwrap();
    emu.step_until_yield(1_000_000).expect("Failed to run until yield");
}

/// Sync client view with server
async fn sync_client_view(
    client: &LpClient,
    handle: lp_model::project::handle::ProjectHandle,
    view: &mut ClientProjectView,
) {
    let detail_spec = view.detail_specifier();
    let response = client.project_sync_internal(
        handle,
        Some(view.frame_id),
        detail_spec,
    ).await.expect("Failed to sync project");

    view.apply_changes(&response.to_serializable().expect("Failed to convert response"))
        .expect("Failed to apply changes");
}
```

### 2. Update Cargo.toml for test (`lp-app/apps/fw-emu/Cargo.toml`)

Add test dependencies:

```toml
[dev-dependencies]
lp-client = { path = "../../../lp-core/lp-client", features = ["serial"] }
lp-engine-client = { path = "../../../lp-core/lp-engine-client" }
lp-riscv-elf = { path = "../../../lp-riscv/lp-riscv-elf" }
lp-riscv-emu = { path = "../../../lp-riscv/lp-riscv-emu" }
lp-shared = { path = "../../crates/lp-shared" }
tokio = { version = "1", features = ["rt", "macros"] }
```

## Notes

- File collection from `LpFsMemory` may need to be implemented based on the actual API
- Output verification is deferred for now (as per Q2 answer)
- The test uses simulated time mode to advance time deterministically
- The test exercises the full message protocol via `lp-client`

## Validate

Run from `lp-app/apps/fw-emu/` directory:

```bash
cd lp-app/apps/fw-emu
cargo test --test scene_render
```

Ensure:

- Test compiles
- Test runs (may need adjustments based on actual APIs)
- No warnings (except for TODO comments)
- Test passes or provides useful failure information
