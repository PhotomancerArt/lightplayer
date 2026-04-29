//! Differential fw-emu test: shader output must vary by `pos`.
//!
//! Mirrors `scene_render_emu` but uses a UV-gradient shader and a fixture
//! with two well-separated lamps. If `pos` is loop-invariant on the device
//! (`lpvm-native` rt_jit) — the suspected bug — both lamps sample the same
//! color and the assertion fails. If `pos` varies correctly per pixel, the
//! lamp at the bottom-right is meaningfully brighter than the one at the
//! top-left.
//!
//! The existing `scene_render_emu` only checks one pixel of a one-lamp
//! fixture, so it's blind to "every pixel is the same color" bugs.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use fw_tests::shader_emu_gate::assert_shader_compiled_ok;
use fw_tests::sync_emu_project_view;
use fw_tests::transport_emu_serial::SerialEmuClientTransport;
use log;
use lp_client::LpClient;
use lp_engine_client::ClientProjectView;
use lp_riscv_elf::load_elf;
use lp_riscv_emu::{
    LogLevel, Riscv32Emulator, TimeMode,
    test_util::{BinaryBuildConfig, ensure_binary_built},
};
use lp_riscv_inst::Gpr;
use lp_shared::ProjectBuilder;
use lpc_model::{AsLpPath, FrameId};
use lpfs::LpFsMemory;
use lpl_model::nodes::fixture::{MappingConfig, PathSpec, RingOrder};

/// UV-gradient shader: red varies with x, green varies with y.
///
/// Pixel-center semantics from the new `__render_texture_*` synth:
///   pos.x runs 0.5, 1.5, ..., (W - 0.5)
///   pos.y runs 0.5, 1.5, ..., (H - 0.5)
/// So for a 4x4 texture, R should range ~0.125..0.875 across columns
/// and G similarly across rows.
const UV_GRADIENT_GLSL: &str = r"
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;
vec4 render(vec2 pos) {
    return vec4(pos.x / outputSize.x, pos.y / outputSize.y, 0.0, 1.0);
}
";

#[tokio::test]
#[test_log::test]
async fn test_scene_render_position_varies_fw_emu() {
    log::info!("Building fw-emu...");
    let fw_emu_path = ensure_binary_built(
        BinaryBuildConfig::new("fw-emu")
            .with_target("riscv32imac-unknown-none-elf")
            .with_profile("release-emu")
            .with_backtrace_support(true),
    )
    .expect("Failed to build fw-emu");

    log::info!("Starting emulator...");

    let elf_data = std::fs::read(&fw_emu_path).expect("Failed to read fw-emu ELF");
    let load_info = load_elf(&elf_data).expect("Failed to load ELF");

    let ram_size = load_info.ram.len();
    let mut emulator = Riscv32Emulator::new(load_info.code, load_info.ram)
        .with_log_level(LogLevel::Instructions)
        .with_time_mode(TimeMode::Simulated(0))
        .with_allow_unaligned_access(true);

    let sp_value = 0x80000000u32.wrapping_add((ram_size as u32).wrapping_sub(16));
    emulator.set_register(Gpr::Sp, sp_value as i32);
    emulator.set_pc(load_info.entry_point);

    let emulator_arc = Arc::new(Mutex::new(emulator));

    let transport = SerialEmuClientTransport::new(emulator_arc.clone())
        .with_backtrace(load_info.symbol_map.clone(), load_info.code_end);

    log::info!("Starting client...");
    let client = LpClient::new(Box::new(transport));

    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());

    // 4x4 texture is enough to make a UV gradient meaningful while staying
    // tiny for the device + emulator to render quickly.
    let texture_path = builder.texture().width(4).height(4).add(&mut builder);

    // Position-varying shader.
    let shader_path = builder
        .shader(&texture_path)
        .glsl(UV_GRADIENT_GLSL)
        .add(&mut builder);

    let output_path = builder.output_basic();

    // Two single-lamp rings sampling far-apart corners of the 4x4 texture.
    // With a small sample diameter this is approximately point-sampling at
    // (0.1, 0.1) and (0.9, 0.9) in normalized texture space, i.e. opposite
    // corners of the gradient.
    builder
        .fixture(&output_path, &texture_path)
        .mapping(MappingConfig::PathPoints {
            paths: vec![
                PathSpec::RingArray {
                    center: (0.1, 0.1),
                    diameter: 0.0,
                    start_ring_inclusive: 0,
                    end_ring_exclusive: 1,
                    ring_lamp_counts: vec![1],
                    offset_angle: 0.0,
                    order: RingOrder::InnerFirst,
                },
                PathSpec::RingArray {
                    center: (0.9, 0.9),
                    diameter: 0.0,
                    start_ring_inclusive: 0,
                    end_ring_exclusive: 1,
                    ring_lamp_counts: vec![1],
                    offset_angle: 0.0,
                    order: RingOrder::InnerFirst,
                },
            ],
            sample_diameter: 0.25,
        })
        .add(&mut builder);

    builder.build();

    let project_files = collect_project_files(&fs.borrow());

    log::info!("Syncing project...");
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
    let project_handle = client
        .project_load(project_dir)
        .await
        .expect("Failed to load project");

    let mut client_view = ClientProjectView::new();
    sync_emu_project_view(&client, project_handle, &mut client_view).await;

    let shader_handle = client_view
        .nodes
        .iter()
        .find(|(_, entry)| entry.path.as_str() == shader_path.as_str())
        .map(|(handle, _)| *handle)
        .expect("Shader node not found in client view");
    client_view.watch_detail(shader_handle);

    let output_handle = client_view
        .nodes
        .iter()
        .find(|(_, entry)| entry.path.as_str() == output_path.as_str())
        .map(|(handle, _)| *handle)
        .expect("Output node not found in client view");
    client_view.watch_detail(output_handle);

    sync_emu_project_view(&client, project_handle, &mut client_view).await;
    assert_shader_compiled_ok(&client_view, shader_path.as_str());

    {
        let mut emu = emulator_arc.lock().unwrap();
        emu.advance_time(40);
    }
    sync_emu_project_view(&client, project_handle, &mut client_view).await;

    let data = client_view
        .get_output_data(output_handle)
        .expect("Failed to get output data");

    assert!(
        data.len() >= 6,
        "Expected at least 6 RGB bytes for 2 lamps; got {} bytes: {:?}",
        data.len(),
        data
    );

    let lamp0 = [data[0], data[1], data[2]];
    let lamp1 = [data[3], data[4], data[5]];

    log::info!("lamp0 (top-left ~0.1,0.1) RGB = {:?}", lamp0);
    log::info!("lamp1 (bot-right ~0.9,0.9) RGB = {:?}", lamp1);

    // The shader writes R = pos.x/W and G = pos.y/H. With pos varying per
    // pixel, lamp1 (bottom-right) should be substantially brighter than
    // lamp0 (top-left) in both R and G. If `pos` is stuck at its initial
    // value (the device-side bug) the two lamps will be ~equal.
    assert!(
        lamp1[0] > lamp0[0] + 16,
        "R should grow with pos.x: lamp0={:?} lamp1={:?} (full data: {:?})",
        lamp0,
        lamp1,
        data
    );
    assert!(
        lamp1[1] > lamp0[1] + 16,
        "G should grow with pos.y: lamp0={:?} lamp1={:?} (full data: {:?})",
        lamp0,
        lamp1,
        data
    );

    assert!(
        client_view.frame_id >= FrameId(1),
        "Should have processed at least 1 frame"
    );
}

/// Collect all files from project filesystem (copy of helper in
/// `scene_render_emu.rs` — kept local to avoid widening the fw-tests
/// public API for a one-off diagnostic).
fn collect_project_files(fs: &LpFsMemory) -> Vec<(String, Vec<u8>)> {
    use lpfs::LpFs;

    let entries = fs
        .list_dir("/".as_path(), true)
        .expect("Failed to list project files");

    let mut files = Vec::new();
    for entry in entries {
        if entry.as_str().ends_with('/') {
            continue;
        }
        if fs.is_dir(entry.as_path()).unwrap_or(false) {
            continue;
        }

        let content = fs
            .read_file(entry.as_path())
            .expect("Failed to read project file");

        let relative_path = if entry.as_str().starts_with('/') {
            &entry.as_str()[1..]
        } else {
            entry.as_str()
        };

        files.push((relative_path.to_string(), content));
    }

    files
}
