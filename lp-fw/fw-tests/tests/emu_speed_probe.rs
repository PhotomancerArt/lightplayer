//! M0 speed probe (ignored, manual): instructions/s of `lp-riscv-emu` running
//! `fw-emu`, next to esp-emu's report numbers
//! (`docs/reports/2026-09-07-esp-emu-c6-spike.md` §7: 68.6 M insns/s idle,
//! 0.14× real time rendering meteor).
//!
//! This is a report-only probe (M0 of the ESP32-C6 emulator plan;
//! `~/.photomancer/planning/lp2025/2026-09-06-1001-esp-emulator/`), not a CI
//! gate — it prints timing and instruction-count windows for
//! `docs/reports/2026-09-06-lp-riscv-emu-speed-probe.md`. Run with:
//!
//! ```text
//! cargo test --release -p fw-tests --test emu_speed_probe -- --ignored --nocapture --test-threads=1
//! ```
//!
//! `--release` matters: without it, `cargo test` builds `lp-riscv-emu`
//! itself (the *host*-side interpreter, not the guest firmware) in the `dev`
//! profile, and the interpreter's own throughput is what this probe
//! measures. The guest firmware (`fw-emu`) is always cross-compiled
//! separately via `ensure_binary_built`, independent of the host test
//! profile; the `release-emu` profile named below applies only to that
//! guest binary.
//!
//! ## Why this harness, and a deviation from the brief worth flagging
//!
//! The brief's primary reference vehicle is `lp-cli ... emu`
//! (`lp-cli/src/client/client_connect.rs`): it builds `fw-emu` with the
//! plain `release` profile and drives it in `TimeMode::RealTime` through
//! the background-thread transport at
//! `lp-app/lpa-client/src/transport_serial/emulator.rs`. Both were tried
//! here and found broken against a real project in this repo state:
//!
//! - Plain `release` `fw-emu` (`lp-cli upload catalog/patterns/meteor emu`, tried
//!   directly): the process pins one CPU core at ~99% with no forward
//!   progress once the first uploaded project file reaches the firmware.
//!   This matches the `release-emu` Cargo.toml comment's warning
//!   ("opt-level=3 avoids Cranelift codegen bugs in emulator
//!   (InvalidMemoryAccess)") almost exactly.
//! - `release-emu` `fw-emu` through the *same* `RealTime` + async-thread
//!   transport (tried as this test's first draft): still times out
//!   (`TokioLpClient`'s own request timeout) on the very first client
//!   round-trip — even a tiny JSON file write, before any project is
//!   loaded, before the JIT ever runs. Confirmed reproducibly with the host
//!   test binary itself built `--release`, ruling out host-interpreter
//!   slowness as the explanation.
//!
//! Both are almost certainly real defects, filed for triage rather than
//! chased down here (M0 is report-only; "no emulator changes"). This probe
//! instead uses the shape `tests/scene_render_emu.rs` already proves works:
//! `release-emu` `fw-emu`, `TimeMode::Simulated`, and the *synchronous*
//! `SerialEmuClientTransport` (`lpa_client::transport_emu_serial`), which
//! only steps the guest when a request is outstanding. That means every
//! window here (including "idle") is driven by a tight loop of lightweight
//! client round-trips rather than true wall-clock-independent background
//! execution — noted per-window below.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fw_tests::transport_emu_serial::SerialEmuClientTransport;
use lp_emu_core::{LogLevel, TimeMode};
use lp_riscv_elf::load_elf;
use lp_riscv_emu::{
    Riscv32Emulator,
    test_util::{BinaryBuildConfig, ensure_binary_built},
};
use lp_riscv_inst::Gpr;
use lpa_client::TokioLpClient;
use lpc_model::AsLpPath;
use lpc_wire::server::api::LogLevel as WireLogLevel;

/// Wall-clock floor for the idle and rendering windows (brief: "≥5 s wall").
const WINDOW_WALL: Duration = Duration::from_secs(5);

#[tokio::test]
#[ignore = "manual M0 speed probe; prints timing for the report, not a CI gate"]
async fn speed_probe_windows() {
    // 1. Build fw-emu (release-emu profile — see module doc).
    let fw_emu_path = ensure_binary_built(
        BinaryBuildConfig::new("fw-emu")
            .with_target("riscv32imac-unknown-none-elf")
            .with_profile("release-emu")
            .with_backtrace_support(true),
    )
    .expect("failed to build fw-emu (release-emu)");

    let elf_data = std::fs::read(&fw_emu_path).expect("failed to read fw-emu ELF");
    let load_info = load_elf(&elf_data).expect("failed to load ELF");
    let ram_size = load_info.ram.len();

    let mut emulator = Riscv32Emulator::new(load_info.code, load_info.ram)
        .with_log_level(LogLevel::None)
        .with_time_mode(TimeMode::Simulated(0))
        .with_allow_unaligned_access(true);
    let sp_value = 0x8000_0000u32.wrapping_add((ram_size as u32).wrapping_sub(16));
    emulator.set_register(Gpr::Sp, sp_value as i32);
    emulator.set_pc(load_info.entry_point);

    let t0 = Instant::now();
    let emulator = Arc::new(Mutex::new(emulator));
    let transport = SerialEmuClientTransport::new(emulator.clone())
        .with_backtrace(load_info.symbol_map.clone(), load_info.code_end);
    let client = TokioLpClient::new(Box::new(transport));

    // --- Window 1: boot -> first served frame -------------------------
    // This transport only steps the guest on a request, so the first
    // round-trip IS the boot: getting a response requires the firmware to
    // have initialized the server and ticked at least once (fw-emu's own
    // `[fw-emu][RECOVERY] boot complete (first frame served)` log line,
    // which fires on the first successful `tick_server_frame` — see
    // `lp-fw/fw-emu/src/server_loop.rs`).
    client
        .set_log_level(WireLogLevel::Info)
        .await
        .expect("boot round-trip (set_log_level) failed");
    let boot_wall = t0.elapsed();
    let boot_instr = emulator.lock().unwrap().get_instruction_count();

    println!(
        "window=boot_to_first_frame wall_ms={} instructions={} insns_per_s={:.0}",
        boot_wall.as_millis(),
        boot_instr,
        boot_instr as f64 / boot_wall.as_secs_f64()
    );

    // --- Window 2: idle serving, no project loaded, >=5s wall ----------
    // No background thread here (synchronous transport): each iteration's
    // `set_log_level` round-trip is what drives the guest forward one tick.
    // Simulated time is not advanced (no project cares yet), so this window
    // is instructions-per-*server-tick* at whatever rate this test's own
    // async loop can issue round-trips, not a wall-clock-independent spin.
    let idle_instr_start = boot_instr;
    let idle_wall_start = Instant::now();
    let mut idle_ticks = 0u64;
    while idle_wall_start.elapsed() < WINDOW_WALL {
        client
            .set_log_level(WireLogLevel::Info)
            .await
            .expect("idle-window round-trip failed");
        idle_ticks += 1;
    }
    let idle_wall = idle_wall_start.elapsed();
    let idle_instr_end = emulator.lock().unwrap().get_instruction_count();
    let idle_instr = idle_instr_end - idle_instr_start;

    println!(
        "window=idle_serving wall_ms={} instructions={} insns_per_s={:.0} ticks={idle_ticks}",
        idle_wall.as_millis(),
        idle_instr,
        idle_instr as f64 / idle_wall.as_secs_f64(),
    );

    // --- Window 3: rendering catalog/patterns/meteor, >=5s wall -----------------
    let project_files = read_project_files("meteor");
    assert!(
        !project_files.is_empty(),
        "catalog/patterns/meteor should have files"
    );
    let deploy_wall_start = Instant::now();
    for (path, bytes) in &project_files {
        client
            .fs_write(format!("/projects/meteor/{path}").as_path(), bytes.clone())
            .await
            .unwrap_or_else(|e| panic!("failed to write project file {path}: {e}"));
    }
    let handle = client
        .project_load("meteor")
        .await
        .expect("failed to load meteor project");

    // advance_time(40) matches `tests/scene_render_emu.rs`'s own tick
    // granularity. Confirm rendering has actually started (first compile is
    // a one-time cost this window should not include) before starting the
    // render window's clock.
    advance_and_wait_for_frame(&client, &emulator, handle).await;
    let deploy_wall = deploy_wall_start.elapsed();
    let render_instr_start = emulator.lock().unwrap().get_instruction_count();
    println!(
        "(project upload + load + first-compile of meteor took {} ms wall, not part of a reported window)",
        deploy_wall.as_millis()
    );

    let render_wall_start = Instant::now();
    let mut render_ticks = 0u32;
    const TICK_MS: u32 = 40;
    while render_wall_start.elapsed() < WINDOW_WALL {
        emulator.lock().unwrap().advance_time(TICK_MS);
        client
            .project_read(
                handle,
                lpc_wire::ProjectReadRequest {
                    since: None,
                    queries: vec![lpc_wire::ProjectReadQuery::Runtime(
                        lpc_wire::RuntimeReadQuery,
                    )],
                    probes: Vec::new(),
                },
            )
            .await
            .expect("project_read failed during render window");
        render_ticks += 1;
    }
    let render_wall = render_wall_start.elapsed();
    let render_instr_end = emulator.lock().unwrap().get_instruction_count();
    let render_instr = render_instr_end - render_instr_start;
    let emulated_ms_advanced = u64::from(render_ticks) * u64::from(TICK_MS);

    println!(
        "window=rendering_meteor wall_ms={} instructions={} insns_per_s={:.0} ticks={render_ticks} \
         emulated_ms_advanced={emulated_ms_advanced} emulated_ms_per_wall_s={:.0} \
         instructions_per_40ms_tick={:.0}",
        render_wall.as_millis(),
        render_instr,
        render_instr as f64 / render_wall.as_secs_f64(),
        emulated_ms_advanced as f64 / render_wall.as_secs_f64(),
        render_instr as f64 / render_ticks as f64,
    );
}

/// Read every file under `catalog/patterns/<project_dir>` from disk, relative paths
/// (forward-slash separated) paired with their bytes.
fn read_project_files(project_dir: &str) -> Vec<(String, Vec<u8>)> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../catalog/patterns")
        .join(project_dir)
        .canonicalize()
        .unwrap_or_else(|e| panic!("{project_dir} should exist: {e}"));

    let mut files = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read_dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let relative = path
                    .strip_prefix(&root)
                    .expect("file should be under root")
                    .to_string_lossy()
                    .replace('\\', "/");
                let bytes = std::fs::read(&path).expect("read project file");
                files.push((relative, bytes));
            }
        }
    }
    files.sort();
    files
}

/// Advance simulated time in 40 ms steps, polling after each, until the
/// project has ticked at least one frame.
async fn advance_and_wait_for_frame(
    client: &TokioLpClient,
    emulator: &Arc<Mutex<Riscv32Emulator>>,
    handle: lpc_wire::WireProjectHandle,
) {
    use lpc_wire::{ProjectReadQuery, ProjectReadRequest, RuntimeReadQuery};

    for _ in 0..200 {
        emulator.lock().unwrap().advance_time(40);
        let events = client
            .project_read(
                handle,
                ProjectReadRequest {
                    since: None,
                    queries: vec![ProjectReadQuery::Runtime(RuntimeReadQuery)],
                    probes: Vec::new(),
                },
            )
            .await
            .expect("project_read failed while waiting for first frame");

        let mut view = lpc_view::ProjectView::new();
        let mut applier = lpc_view::ProjectReadApplier::new(&mut view);
        for event in events {
            applier.apply(event).expect("apply project read event");
        }
        if view
            .runtime
            .as_ref()
            .map(|r| r.project.frame_num > 0)
            .unwrap_or(false)
        {
            return;
        }
    }
    panic!("meteor did not report a rendered frame within the poll budget");
}
