//! Repro for `docs/defects/2026-08-02-c6-project-outgrows-board-outputs-oom-storm.md`.
//!
//! A project that declares more WS281x outputs than the board has timing
//! channels must degrade to the outputs that exist. The C6 field report was a
//! 2-output project on a 1-channel manifest; the emulator's virtual board
//! declares four timing resources and 256 GPIO pads, so a **five**-output
//! project is the same shape with no firmware edit: five valid endpoints,
//! four channels, one open that can never succeed.
//!
//! This test is the instrumentation the defect asks for before any fix: it
//! runs the healthy (4-output) and overshooting (5-output) projects through
//! the real firmware image and counts what the fifth sink costs per frame.
//!
//! What it pins is the *degradation contract*, which outlived the storm it was
//! written for: the rv32 firmwares are abort tier now (ADR
//! `2026-08-02-rv32-firmwares-are-abort-tier`), so a caught-panic storm is
//! unreachable by construction — but an endpoint that cannot open must still
//! cost the board one sink and not its boot, and must not ratchet the heap.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};

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
use lpc_shared::ProjectBuilder;
use lpfs::{LpFs, LpFsMemory};

/// Frames to run after the project is loaded.
const FRAMES: usize = 40;

// ---------------------------------------------------------------------------
// Guest log capture
// ---------------------------------------------------------------------------

static CAPTURE: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();

struct CapturingLogger;

impl log::Log for CapturingLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if let Some(sink) = CAPTURE.get() {
            sink.lock()
                .unwrap()
                .push(format!("{} {}", record.level(), record.args()));
        }
    }

    fn flush(&self) {}
}

fn capture() -> Arc<Mutex<Vec<String>>> {
    CAPTURE
        .get_or_init(|| {
            log::set_boxed_logger(Box::new(CapturingLogger)).expect("install capturing logger");
            log::set_max_level(log::LevelFilter::Debug);
            Arc::new(Mutex::new(Vec::new()))
        })
        .clone()
}

// ---------------------------------------------------------------------------

#[tokio::test]
async fn project_outgrowing_board_outputs_degrades_without_per_frame_cost() {
    let sink = capture();

    let healthy = run_project(&sink, 4).await;
    let overshoot = run_project(&sink, 5).await;

    println!("\n================ OUTPUT OVERSHOOT REPRO ================");
    println!("healthy   (4 outputs, 4 channels): {healthy:#?}");
    println!("overshoot (5 outputs, 4 channels): {overshoot:#?}");
    println!("=======================================================\n");

    assert!(
        overshoot.frames_ticked > 0,
        "the overshooting project must still run frames; it produced none"
    );

    // The defect's core question: is the failing endpoint retried every frame?
    assert!(
        overshoot.open_failures <= healthy.open_failures + 2,
        "the unopenable sink was retried per frame: {} open failures over {} frames \
         (healthy project logged {}). A parked sink must be asked once, not once per frame.",
        overshoot.open_failures,
        overshoot.frames_ticked,
        healthy.open_failures,
    );

    assert_eq!(
        overshoot.panics, 0,
        "a project that outgrows the board must not panic; caught panics: {:?}",
        overshoot.panic_lines
    );

    // The defect's first question: leak, or bounded transient? The failing
    // sink must not ratchet the heap upward frame after frame.
    assert!(
        overshoot.heap_samples >= FRAMES / 2,
        "heap reporting did not run ({} samples); the measurement below is meaningless",
        overshoot.heap_samples
    );
    assert!(
        overshoot.heap_growth <= 4096,
        "heap grew {} bytes over {} steady-state frames with an unopenable sink \
         (healthy project grew {}): the failed-open path leaks.",
        overshoot.heap_growth,
        overshoot.heap_samples,
        healthy.heap_growth,
    );
}

#[derive(Debug)]
#[allow(dead_code)]
struct RunStats {
    outputs: usize,
    frames_ticked: u64,
    total_log_lines: usize,
    heap_samples: usize,
    heap_used_first: usize,
    heap_used_last: usize,
    heap_used_max: usize,
    heap_growth: usize,
    open_failures: usize,
    panics: usize,
    oom_lines: usize,
    panic_lines: Vec<String>,
    sample_failure_lines: Vec<String>,
}

async fn run_project(sink: &Arc<Mutex<Vec<String>>>, outputs: usize) -> RunStats {
    sink.lock().unwrap().clear();

    let fw_emu_path = ensure_binary_built(
        BinaryBuildConfig::new("fw-emu")
            .with_target("riscv32imac-unknown-none-elf")
            .with_profile("release-emu")
            .with_backtrace_support(true)
            .with_features(&["heap_report"]),
    )
    .expect("Failed to build fw-emu");

    let elf_data = std::fs::read(&fw_emu_path).expect("Failed to read fw-emu ELF");
    let load_info = load_elf(&elf_data).expect("Failed to load ELF");
    let ram_size = load_info.ram.len();
    let mut emulator = Riscv32Emulator::new(load_info.code, load_info.ram)
        .with_log_level(LogLevel::None)
        .with_time_mode(TimeMode::Simulated(0))
        .with_allow_unaligned_access(true);

    let sp_value = 0x80000000u32.wrapping_add((ram_size as u32).wrapping_sub(16));
    emulator.set_register(Gpr::Sp, sp_value as i32);
    emulator.set_pc(load_info.entry_point);

    let emulator = Arc::new(Mutex::new(emulator));
    let transport = SerialEmuClientTransport::new(emulator.clone())
        .with_backtrace(load_info.symbol_map.clone(), load_info.code_end);
    let client = TokioLpClient::new(Box::new(transport));

    // Four labelled pads the virtual quad board carries, plus a plain GPIO for
    // the fifth: five distinct, valid WS281x endpoints over four channels.
    const PADS: [&str; 5] = ["D10", "D9", "D8", "D7", "GPIO20"];

    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());
    builder.clock_basic();
    let texture_path = builder.texture().width(2).height(2).add(&mut builder);
    builder.shader_basic(&texture_path);
    // One fixture drives the control bus; every output reads it. Several
    // fixtures targeting one channel is an ambiguous binding, but several
    // outputs reading it is not — and each output still registers its own
    // sink against its own endpoint, which is what this test is about.
    let mut first_output = None;
    for pad in PADS.iter().take(outputs) {
        let output_path = builder
            .output()
            .endpoint_str(&format!("ws281x:rmt:{pad}"))
            .add(&mut builder);
        first_output.get_or_insert(output_path);
    }
    let first_output = first_output.expect("at least one output");
    builder.fixture_basic(&first_output, &texture_path);
    builder.build();

    let project_dir = "project";
    for (path, content) in collect_project_files(&fs.borrow()) {
        client
            .fs_write(format!("/projects/{project_dir}/{path}").as_path(), content)
            .await
            .expect("Failed to write project file");
    }

    let project_handle = client
        .project_load(project_dir)
        .await
        .expect("Failed to load project");

    // Everything logged from here on is steady-state, per-frame behaviour.
    sink.lock().unwrap().clear();

    // `advance_time` only moves the clock; the emulator executes while it is
    // servicing a request, so each frame needs a read to pump it.
    let mut frames_ticked = 0;
    for _ in 0..FRAMES {
        emulator.lock().unwrap().advance_time(40);
        frames_ticked = read_frame_num(&client, project_handle).await;
    }

    let lines = sink.lock().unwrap().clone();
    if std::env::var("DUMP_LOG").is_ok() {
        let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
        for line in &lines {
            *counts.entry(line.clone()).or_default() += 1;
        }
        println!("---- distinct log lines ({outputs} outputs) ----");
        for (line, n) in &counts {
            println!("  [{n:>4}x] {line}");
        }
    }
    // `EngineServices: output <endpoint>: <error>` is the engine's per-sink
    // flush/open failure line — one per sink per frame it is actually asked.
    let is_open_failure = |line: &&String| line.contains("EngineServices: output ");
    let open_failures = lines.iter().filter(is_open_failure).count();
    let panic_lines: Vec<String> = lines
        .iter()
        .filter(|l| l.to_ascii_lowercase().contains("panic"))
        .cloned()
        .collect();
    let oom_lines = lines
        .iter()
        .filter(|l| l.to_ascii_lowercase().contains("oom") || l.contains("allocation failed"))
        .count();

    // `[heap] used=N free=M`, one per frame.
    let heap_used: Vec<usize> = lines
        .iter()
        .filter_map(|line| {
            let rest = line.split("[heap] used=").nth(1)?;
            rest.split_whitespace().next()?.parse().ok()
        })
        .collect();

    RunStats {
        outputs,
        frames_ticked,
        total_log_lines: lines.len(),
        heap_samples: heap_used.len(),
        heap_used_first: heap_used.first().copied().unwrap_or(0),
        heap_used_last: heap_used.last().copied().unwrap_or(0),
        heap_used_max: heap_used.iter().copied().max().unwrap_or(0),
        heap_growth: heap_used
            .last()
            .copied()
            .unwrap_or(0)
            .saturating_sub(heap_used.first().copied().unwrap_or(0)),
        open_failures,
        panics: panic_lines.len(),
        oom_lines,
        sample_failure_lines: lines
            .iter()
            .filter(is_open_failure)
            .take(4)
            .cloned()
            .collect(),
        panic_lines,
    }
}

async fn read_frame_num(client: &TokioLpClient, handle: lpc_wire::WireProjectHandle) -> u64 {
    use lpc_view::{ApplyStatus, ProjectReadApplier, ProjectView};
    use lpc_wire::{ProjectReadQuery, ProjectReadRequest, RuntimeReadQuery};

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
        .expect("runtime read should succeed");

    let mut view = ProjectView::new();
    let mut applier = ProjectReadApplier::new(&mut view);
    for event in events {
        if let ApplyStatus::Complete { .. } = applier.apply(event).expect("apply runtime read") {
            break;
        }
    }
    view.runtime
        .as_ref()
        .expect("runtime status present")
        .project
        .frame_num
}

fn collect_project_files(fs: &LpFsMemory) -> Vec<(String, Vec<u8>)> {
    let entries = fs
        .list_dir("/".as_path(), true)
        .expect("Failed to list project files");

    let mut files = Vec::new();
    for entry in entries {
        if entry.as_str().ends_with('/') || fs.is_dir(entry.as_path()).unwrap_or(false) {
            continue;
        }
        let content = fs
            .read_file(entry.as_path())
            .expect("Failed to read project file");
        files.push((entry.as_str().trim_start_matches('/').to_string(), content));
    }
    files
}
