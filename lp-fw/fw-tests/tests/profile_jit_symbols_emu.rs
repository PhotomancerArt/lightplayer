//! End-to-end: guest `SYSCALL_JIT_MAP_LOAD` → `meta.json` `dynamic_symbols` → CLI symbolizer.
//!
//! Boots `fw-emu` with the `profile` feature (slow). Run:
//!
//!     cargo test -p fw-tests --test profile_jit_symbols_emu

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use fw_tests::shader_emu_gate::assert_shader_compiled_ok;
use fw_tests::sync_emu_project_view;
use fw_tests::transport_emu_serial::SerialEmuClientTransport;
use log;
use lp_cli::commands::profile::symbolize::symbolizer_from_meta_json_str;
use lp_client::LpClient;
use lp_engine_client::ClientProjectView;
use lp_model::AsLpPath;
use lp_riscv_elf::load_elf;
use lp_riscv_emu::{
    LogLevel, Riscv32Emulator, TimeMode,
    profile::{SessionMetadata, TraceSymbol},
    test_util::{BinaryBuildConfig, ensure_binary_built},
};
use lp_riscv_inst::Gpr;
use lp_shared::ProjectBuilder;
use lp_shared::fs::LpFsMemory;

/// `ProjectBuilder::shader_basic` uses GLSL with entry point `render` (see `lp-shader`).
const EXPECTED_JIT_FN: &str = "render";

const FRAMES: u32 = 3;

#[tokio::test]
#[test_log::test]
async fn jit_symbols_round_trip_to_meta_and_symbolizer() {
    log::info!("Building fw-emu with profile...");
    let fw_emu_path = ensure_binary_built(
        BinaryBuildConfig::new("fw-emu")
            .with_target("riscv32imac-unknown-none-elf")
            .with_profile("release-emu")
            .with_backtrace_support(true)
            .with_features(&["profile"]),
    )
    .expect("Failed to build fw-emu with profile");

    let elf_data = std::fs::read(&fw_emu_path).expect("Failed to read fw-emu ELF");
    let load_info = load_elf(&elf_data).expect("Failed to load ELF");

    let trace_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let trace_path = trace_dir.path().to_path_buf();

    let metadata = SessionMetadata {
        schema_version: 1,
        timestamp: "2026-01-01T00:00:00Z".into(),
        project: "fw-tests".into(),
        workload: "profile-jit-symbols-emu".into(),
        note: None,
        clock_source: "emu_estimated",
        mode: "steady-render".into(),
        cycle_model: "esp32c6".into(),
        max_cycles: u64::MAX,
        cycles_used: 0,
        terminated_by: "running".into(),
        symbols: load_info
            .symbol_list
            .iter()
            .map(|s| TraceSymbol {
                name: s.name.clone(),
                addr: s.addr,
                size: s.size,
            })
            .collect(),
    };

    let ram_size = load_info.ram.len();
    let mut emulator = Riscv32Emulator::new(load_info.code, load_info.ram)
        .with_log_level(LogLevel::Instructions)
        .with_time_mode(TimeMode::Simulated(0))
        .with_allow_unaligned_access(true)
        .with_profile_session(trace_path.clone(), &metadata, vec![])
        .expect("Failed to enable profile session");

    let sp_value = 0x8000_0000u32.wrapping_add((ram_size as u32).wrapping_sub(16));
    emulator.set_register(Gpr::Sp, sp_value as i32);
    emulator.set_pc(load_info.entry_point);

    let emulator_arc = Arc::new(Mutex::new(emulator));

    let transport = SerialEmuClientTransport::new(emulator_arc.clone())
        .with_backtrace(load_info.symbol_map.clone(), load_info.code_end);

    let client = LpClient::new(Box::new(transport));

    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());
    let texture_path = builder.texture().width(2).height(2).add(&mut builder);
    let shader_path = builder.shader_basic(&texture_path);
    let output_path = builder.output_basic();
    builder.fixture_basic(&output_path, &texture_path);
    builder.build();

    let project_files = collect_project_files(&fs.borrow());
    let project_dir = "project";
    for (path, content) in project_files {
        let full_path = format!("/projects/{}/{}", project_dir, path);
        client
            .fs_write(full_path.as_path(), content)
            .await
            .expect("Failed to write project file");
    }

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
    sync_emu_project_view(&client, project_handle, &mut client_view).await;
    assert_shader_compiled_ok(&client_view, shader_path.as_str());

    for _ in 0..FRAMES {
        let mut emu = emulator_arc.lock().unwrap();
        emu.advance_time(40);
    }

    client
        .stop_all_projects()
        .await
        .expect("Failed to stop projects");

    emulator_arc
        .lock()
        .unwrap()
        .finish_profile_session()
        .expect("Failed to finish profile session");

    let meta_path = trace_path.join("meta.json");
    let meta_content = std::fs::read_to_string(&meta_path).expect("read meta.json");
    let meta: serde_json::Value = serde_json::from_str(&meta_content).expect("parse meta.json");

    let dynamic = meta["dynamic_symbols"]
        .as_array()
        .expect("dynamic_symbols present and array");
    assert!(!dynamic.is_empty(), "dynamic_symbols should be non-empty");

    let entry = dynamic
        .iter()
        .find(|e| e["name"].as_str() == Some(EXPECTED_JIT_FN))
        .unwrap_or_else(|| {
            panic!(
                "{EXPECTED_JIT_FN} not in dynamic_symbols; have names: {:?}",
                dynamic
                    .iter()
                    .filter_map(|e| e["name"].as_str())
                    .collect::<Vec<_>>()
            )
        });

    let addr = entry["addr"].as_u64().expect("addr");
    let size = entry["size"].as_u64().expect("size");
    assert!(size > 0, "JIT symbol size should be positive");
    let pc = (addr + size / 2) as u32;
    assert!(
        (addr..addr.saturating_add(size)).contains(&(pc as u64)),
        "sample PC should fall in [addr, addr+size)"
    );

    let symbolizer = symbolizer_from_meta_json_str(&meta_content).expect("symbolizer from meta");
    let expected_display = format!("[jit] {EXPECTED_JIT_FN}");
    assert_eq!(
        symbolizer.lookup(pc).as_ref(),
        expected_display,
        "Symbolizer should resolve mid-function PC to JIT name with [jit] prefix"
    );
}

fn collect_project_files(fs: &LpFsMemory) -> Vec<(String, Vec<u8>)> {
    use lp_shared::fs::LpFs;

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
