//! Slow integration tests that boot the full `fw-emu` firmware stack.
//!
//! These are gated behind `#[ignore]` because they take minutes per test
//! once per-instruction CPU profiling is enabled, which made `cargo test`
//! mainline runs unbearable. Run explicitly with:
//!
//!     cargo test -p fw-tests --test profile_alloc_emu -- --include-ignored
//!
//! See docs/roadmaps/2026-04-19-cpu-profile/m6-validation-docs.md for the
//! testing-strategy decision recorded 2026-04-19.
//!
//! Integration test: verify allocation profiling produces valid output.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use fw_tests::shader_emu_gate::assert_shader_compiled_ok;
use fw_tests::sync_emu_project_view;
use fw_tests::transport_emu_serial::SerialEmuClientTransport;
use log;
use lp_client::LpClient;
use lp_riscv_elf::load_elf;
use lp_riscv_emu::{
    LogLevel, Riscv32Emulator, TimeMode,
    profile::alloc::AllocCollector,
    profile::{Collector, SessionMetadata, TraceSymbol},
    test_util::{BinaryBuildConfig, ensure_binary_built},
};
use lp_riscv_inst::Gpr;
use lp_shared::ProjectBuilder;
use lpc_model::AsLpPath;
use lpc_view::ClientProjectView;
use lpfs::LpFsMemory;

const FRAMES: u32 = 3;
const HEAP_START: u32 = 0x8000_0000;
const HEAP_SIZE: u32 = 256 * 1024;

#[tokio::test]
#[test_log::test]
#[ignore = "boots fw-emu; slow with profile feature — run explicitly with `cargo test -- --ignored` or `--include-ignored`"]
async fn test_profile_alloc_produces_valid_output() {
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
        workload: "profile-alloc-emu".into(),
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

    let alloc: Box<dyn Collector> = Box::new(
        AllocCollector::new(trace_path.as_path(), HEAP_START, HEAP_SIZE)
            .expect("AllocCollector::new"),
    );

    let ram_size = load_info.ram.len();
    let mut emulator = Riscv32Emulator::new(load_info.code, load_info.ram)
        .with_log_level(LogLevel::Instructions)
        .with_time_mode(TimeMode::Simulated(0))
        .with_allow_unaligned_access(true)
        .with_profile_session(trace_path.clone(), &metadata, vec![alloc])
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

    let counts = {
        let mut emu = emulator_arc.lock().unwrap();
        emu.finish_profile_session()
            .expect("Failed to finish profile session")
    };

    let event_count = counts
        .iter()
        .find(|(name, _)| name == "alloc")
        .map(|(_, c)| *c)
        .unwrap_or(0);

    log::info!("Trace produced {} events", event_count);
    assert!(event_count > 0, "Should have recorded allocation events");

    let meta_path = trace_path.join("meta.json");
    assert!(meta_path.exists(), "meta.json should exist");
    let meta_content = std::fs::read_to_string(&meta_path).expect("Failed to read meta.json");
    let meta: serde_json::Value =
        serde_json::from_str(&meta_content).expect("meta.json should be valid JSON");
    assert_eq!(meta["schema_version"], 1);
    assert!(meta["mode"].as_str().is_some());
    assert!(meta["max_cycles"].as_u64().is_some());
    assert!(meta["cycles_used"].as_u64().is_some());
    assert!(meta["terminated_by"].as_str().is_some());
    assert!(
        meta.get("frames_requested").is_none(),
        "frames_requested removed in m1"
    );
    assert!(
        meta["symbols"].as_array().unwrap().len() > 0,
        "Should have symbols"
    );
    assert!(
        meta["collectors"]["alloc"].is_object(),
        "collectors.alloc should be present"
    );
    assert_eq!(
        meta["collectors"]["alloc"]["heap_start"].as_u64(),
        Some(HEAP_START as u64)
    );
    assert_eq!(
        meta["collectors"]["alloc"]["heap_size"].as_u64(),
        Some(HEAP_SIZE as u64)
    );

    let report_path = trace_path.join("report.txt");
    assert!(report_path.exists(), "report.txt should exist");
    let report = std::fs::read_to_string(&report_path).expect("read report.txt");
    assert!(!report.is_empty(), "report.txt should be non-empty");

    let trace_file_path = trace_path.join("heap-trace.jsonl");
    assert!(trace_file_path.exists(), "heap-trace.jsonl should exist");
    let trace_content =
        std::fs::read_to_string(&trace_file_path).expect("Failed to read heap-trace.jsonl");
    let lines: Vec<&str> = trace_content.lines().collect();
    assert!(
        !lines.is_empty(),
        "heap-trace.jsonl should have at least one event"
    );

    let mut has_alloc = false;
    let mut has_dealloc = false;
    let mut prev_ic = 0u64;

    for line in &lines {
        let event: serde_json::Value =
            serde_json::from_str(line).expect("Each line should be valid JSON");
        let t = event["t"].as_str().expect("Event should have 't' field");
        assert!(
            t == "A" || t == "D" || t == "R",
            "Event type should be A, D, or R, got: {}",
            t
        );
        assert!(event["ptr"].is_u64(), "Event should have 'ptr' field");
        assert!(event["sz"].is_u64(), "Event should have 'sz' field");

        let ic = event["ic"].as_u64().expect("Event should have 'ic' field");
        assert!(ic >= prev_ic, "Instruction counts should be non-decreasing");
        prev_ic = ic;

        let frames = event["frames"]
            .as_array()
            .expect("Event should have 'frames' array");
        assert!(
            !frames.is_empty(),
            "Stack frames should not be empty (event type: {})",
            t
        );

        assert!(event["free"].is_u64(), "Event should have 'free' field");

        if t == "A" {
            has_alloc = true;
        }
        if t == "D" {
            has_dealloc = true;
        }
    }

    assert!(has_alloc, "Should have at least one alloc event");
    assert!(has_dealloc, "Should have at least one dealloc event");

    log::info!(
        "Profile alloc test passed: {} events, {} lines in trace file",
        event_count,
        lines.len()
    );
}

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
