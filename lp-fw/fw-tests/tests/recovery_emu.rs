//! Crash-recovery integration tests against fw-emu.
//!
//! The harness plays the role of the hardware: it preserves the recovery
//! region across simulated reboots (the RTC-fast-RAM analog), supplies the
//! reset cause (the reset-reason register analog), injects faults by
//! writing the handshake words in guest RAM, and treats fuel exhaustion as
//! the hardware watchdog.
//!
//! Guest-side flow under test is the REAL machinery: `panic!` → guest
//! panic handler (stages breadcrumb) → `unwinding` → engine catch boundary
//! (`catch_node_panic_framed`) → blame ledger; or, for uncaught faults,
//! finalize-breadcrumb → reset request sentinel.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use fw_tests::transport_emu_serial::SerialEmuClientTransport;
use lp_recovery::{CrashCause, RecoveryLevel, RecoveryRegion, RecoverySnapshot};
use lp_riscv_elf::load_elf;
use lp_riscv_emu::{
    EmulatorError, LogLevel, Riscv32Emulator, TimeMode,
    test_util::{BinaryBuildConfig, ensure_binary_built},
};
use lp_riscv_emu_shared::recovery_handshake as hs;
use lp_riscv_inst::Gpr;
use lpa_client::TokioLpClient;
use lpc_model::{AsLpPath, NodeId, NodeRuntimeStatus};
use lpc_shared::ProjectBuilder;
use lpc_view::{ApplyStatus, ProjectReadApplier, ProjectView};
use lpc_wire::{
    NodeReadQuery, ProjectProbeRequest, ProjectProbeResult, ProjectReadQuery, ProjectReadRequest,
    ReadLevel, RenderProductProbeRequest, RenderProductProbeResult, RuntimeReadQuery,
    WireTextureFormat,
};
use lpfs::{LpFs, LpFsMemory};

const RAM_BASE: u32 = 0x8000_0000;
const FRAME_FUEL: u64 = 500_000_000;
/// Small budget for hang detection — the "hardware watchdog" of the tests.
const HANG_FUEL: u64 = 20_000_000;

#[derive(Debug, PartialEq, Eq)]
enum RunOutcome {
    Yielded,
    /// Guest requested a reset (crash path committed a breadcrumb).
    ResetRequested,
    /// Fuel ran out — the watchdog analog fired.
    FuelExhausted,
}

struct RecoveryEmuHarness {
    elf_data: Vec<u8>,
    area_addr: u32,
    emulator: Option<Riscv32Emulator>,
    saved_region: Option<Vec<u8>>,
}

impl RecoveryEmuHarness {
    fn new() -> Self {
        let fw_emu_path = ensure_binary_built(
            BinaryBuildConfig::new("fw-emu")
                .with_target("riscv32imac-unknown-none-elf")
                .with_profile("release-emu")
                .with_backtrace_support(true)
                .with_unwind_support(true)
                .with_build_std(true),
        )
        .expect("Failed to build fw-emu");
        let elf_data = std::fs::read(&fw_emu_path).expect("Failed to read fw-emu ELF");
        let load_info = load_elf(&elf_data).expect("Failed to load ELF");
        let area_addr = *load_info
            .symbol_map
            .get(hs::RECOVERY_AREA_SYMBOL)
            .unwrap_or_else(|| {
                panic!(
                    "symbol {} not found in fw-emu symbol map ({} symbols)",
                    hs::RECOVERY_AREA_SYMBOL,
                    load_info.symbol_map.len()
                )
            });
        assert!(area_addr >= RAM_BASE, "recovery area must live in RAM");
        Self {
            elf_data,
            area_addr,
            emulator: None,
            saved_region: None,
        }
    }

    /// Start a fresh guest run with the given reset cause. Restores the
    /// previously saved region bytes when `preserve_region` (the RTC-RAM
    /// analog); a power-on boot passes `false` for true cold-start RAM.
    fn boot(&mut self, cause: u32, preserve_region: bool) {
        let load_info = load_elf(&self.elf_data).expect("Failed to load ELF");
        let ram_size = load_info.ram.len();
        let mut emulator = Riscv32Emulator::new(load_info.code, load_info.ram)
            .with_log_level(LogLevel::None)
            .with_time_mode(TimeMode::Simulated(0))
            .with_allow_unaligned_access(true);
        let sp_value = RAM_BASE.wrapping_add((ram_size as u32).wrapping_sub(16));
        emulator.set_register(Gpr::Sp, sp_value as i32);
        emulator.set_pc(load_info.entry_point);
        self.emulator = Some(emulator);

        if preserve_region {
            let saved = self
                .saved_region
                .clone()
                .expect("no saved region to preserve");
            self.write_bytes(self.area_addr + hs::REGION_OFFSET as u32, &saved);
        }
        self.write_u32(self.area_addr + hs::RESET_CAUSE_OFFSET as u32, cause);
        self.write_u32(self.area_addr + hs::FAULT_REQUEST_OFFSET as u32, 0);
        self.write_u32(self.area_addr + hs::FAULT_ARG_OFFSET as u32, 0);
        self.write_u32(self.area_addr + hs::FAULT_RESULT_OFFSET as u32, 0);
    }

    /// Save the current region bytes (called when a run ends in a crash or
    /// watchdog event, before rebooting).
    fn save_region(&mut self) {
        let bytes = self.read_bytes(
            self.area_addr + hs::REGION_OFFSET as u32,
            RecoveryRegion::SIZE,
        );
        self.saved_region = Some(bytes);
    }

    /// Crash-path reboot: preserve the region and boot with `cause`.
    fn reboot_preserving(&mut self, cause: u32) {
        self.save_region();
        self.boot(cause, true);
    }

    fn set_fault(&mut self, code: u32, arg: u32) {
        self.write_u32(self.area_addr + hs::FAULT_RESULT_OFFSET as u32, 0);
        self.write_u32(self.area_addr + hs::FAULT_ARG_OFFSET as u32, arg);
        self.write_u32(self.area_addr + hs::FAULT_REQUEST_OFFSET as u32, code);
    }

    fn fault_result(&mut self) -> u32 {
        self.read_u32(self.area_addr + hs::FAULT_RESULT_OFFSET as u32)
    }

    /// Run one guest frame (until yield) with the normal fuel budget.
    fn run_frame(&mut self) -> RunOutcome {
        self.run_with_fuel(FRAME_FUEL)
    }

    fn run_with_fuel(&mut self, fuel: u64) -> RunOutcome {
        let emulator = self.emulator.as_mut().expect("no emulator booted");
        emulator.advance_time(20);
        match emulator.run_until_yield(fuel) {
            Ok(_) => RunOutcome::Yielded,
            Err(EmulatorError::Panic { info, .. }) => {
                if info.message == hs::RESET_REQUEST_SENTINEL {
                    RunOutcome::ResetRequested
                } else {
                    panic!("unexpected guest panic escaped to host: {}", info.message);
                }
            }
            Err(EmulatorError::InstructionLimitExceeded { .. }) => RunOutcome::FuelExhausted,
            Err(error) => panic!("unexpected emulator error: {error:?}"),
        }
    }

    /// Run frames until the boot-complete milestone is set (server alive).
    fn run_until_boot_complete(&mut self) {
        for _ in 0..20 {
            assert_eq!(self.run_frame(), RunOutcome::Yielded);
            if self.snapshot().boot_complete {
                return;
            }
        }
        panic!("guest never reached boot-complete");
    }

    /// Inspect the recovery region as the guest sees it right now.
    fn snapshot(&mut self) -> RecoverySnapshot {
        let bytes = self.read_bytes(
            self.area_addr + hs::REGION_OFFSET as u32,
            RecoveryRegion::SIZE,
        );
        RecoveryRegion::read_from_bytes(&bytes)
            .expect("region bytes")
            .inspect()
    }

    // --- raw guest-RAM access ---------------------------------------------

    fn ram_offset(&self, addr: u32) -> usize {
        (addr - RAM_BASE) as usize
    }

    fn read_bytes(&mut self, addr: u32, len: usize) -> Vec<u8> {
        let offset = self.ram_offset(addr);
        let emulator = self.emulator.as_mut().expect("no emulator booted");
        emulator.memory().ram()[offset..offset + len].to_vec()
    }

    fn write_bytes(&mut self, addr: u32, bytes: &[u8]) {
        let offset = self.ram_offset(addr);
        let emulator = self.emulator.as_mut().expect("no emulator booted");
        emulator.memory_mut().ram_mut()[offset..offset + bytes.len()].copy_from_slice(bytes);
    }

    fn read_u32(&mut self, addr: u32) -> u32 {
        let bytes = self.read_bytes(addr, 4);
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    fn write_u32(&mut self, addr: u32, value: u32) {
        self.write_bytes(addr, &value.to_le_bytes());
    }
}

/// Helper: run a fault and return (outcome, fault_result).
fn inject(harness: &mut RecoveryEmuHarness, code: u32, arg: u32) -> (RunOutcome, u32) {
    harness.set_fault(code, arg);
    let outcome = harness.run_frame();
    let result = harness.fault_result();
    (outcome, result)
}

fn entry_states(snapshot: &RecoverySnapshot) -> Vec<(String, &'static str)> {
    snapshot
        .path_entries
        .iter()
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            (
                entry.name().to_string(),
                if entry.is_red() { "red" } else { "yellow" },
            )
        })
        .collect()
}

#[test_log::test]
#[ignore = "slow emulator suite; run via `just test-recovery-emu`"]
fn baseline_boot_is_green_and_completes() {
    let mut harness = RecoveryEmuHarness::new();
    harness.boot(hs::CAUSE_POWER_ON, false);
    harness.run_until_boot_complete();
    let snapshot = harness.snapshot();
    assert_eq!(snapshot.level, RecoveryLevel::Green);
    assert!(snapshot.last_crash.is_none());
    assert!(!snapshot.safe_mode);
    assert_eq!(snapshot.boot_count, 1);
}

#[test_log::test]
#[ignore = "slow emulator suite; run via `just test-recovery-emu`"]
fn recovered_panics_gate_then_power_on_clears() {
    let mut harness = RecoveryEmuHarness::new();
    harness.boot(hs::CAUSE_POWER_ON, false);
    harness.run_until_boot_complete();

    // Crash 1 (caught in-process): path goes yellow, server keeps running.
    let (outcome, result) = inject(&mut harness, hs::FAULT_RECOVERED_PANIC, 1);
    assert_eq!(
        outcome,
        RunOutcome::Yielded,
        "caught panic must not kill the guest"
    );
    assert_eq!(result, hs::FAULT_RESULT_ERROR);
    let snapshot = harness.snapshot();
    assert_eq!(snapshot.level, RecoveryLevel::Yellow);
    assert!(
        entry_states(&snapshot)
            .iter()
            .any(|(name, state)| name == "fault/a" && *state == "yellow"),
        "entries: {:?}",
        entry_states(&snapshot)
    );

    // Crash 2 on the same path: red.
    let (outcome, result) = inject(&mut harness, hs::FAULT_RECOVERED_PANIC, 1);
    assert_eq!(outcome, RunOutcome::Yielded);
    assert_eq!(result, hs::FAULT_RESULT_ERROR);
    assert_eq!(harness.snapshot().level, RecoveryLevel::Red);

    // Third attempt: gated up front — the fault body never runs.
    let (outcome, result) = inject(&mut harness, hs::FAULT_RECOVERED_PANIC, 1);
    assert_eq!(outcome, RunOutcome::Yielded);
    assert_eq!(result, hs::FAULT_RESULT_GATED);

    // OOM-shaped crash on a sibling still works (sibling unaffected by gate).
    let (outcome, result) = inject(&mut harness, hs::FAULT_OOM_PANIC, 2);
    assert_eq!(outcome, RunOutcome::Yielded);
    assert_eq!(result, hs::FAULT_RESULT_ERROR);

    // Power-on clears everything even with region bytes preserved:
    // the reset cause alone invalidates them.
    harness.reboot_preserving(hs::CAUSE_POWER_ON);
    harness.run_until_boot_complete();
    let snapshot = harness.snapshot();
    assert_eq!(snapshot.level, RecoveryLevel::Green);
    assert!(snapshot.last_crash.is_none());
}

#[test_log::test]
#[ignore = "slow emulator suite; run via `just test-recovery-emu`"]
fn escalation_gates_the_parent_and_clean_runs_return_to_green() {
    let mut harness = RecoveryEmuHarness::new();
    harness.boot(hs::CAUSE_POWER_ON, false);
    harness.run_until_boot_complete();

    // Crashes under two DISTINCT children of fault-parent.
    let (_, result) = inject(&mut harness, hs::FAULT_RECOVERED_PANIC, 1);
    assert_eq!(result, hs::FAULT_RESULT_ERROR);
    let (_, result) = inject(&mut harness, hs::FAULT_RECOVERED_PANIC, 2);
    assert_eq!(result, hs::FAULT_RESULT_ERROR);

    // Parent is gated: even an untouched child is denied.
    let (outcome, result) = inject(&mut harness, hs::FAULT_RECOVERED_PANIC, 3);
    assert_eq!(outcome, RunOutcome::Yielded);
    assert_eq!(
        result,
        hs::FAULT_RESULT_GATED,
        "parent escalation gates new children"
    );
    let snapshot = harness.snapshot();
    assert!(
        entry_states(&snapshot)
            .iter()
            .any(|(name, state)| name == "fault-parent" && *state == "red"),
        "entries: {:?}",
        entry_states(&snapshot)
    );

    // Next boot demotes red to yellow (one retry per boot); clean runs of
    // the previously-crashing paths eventually clear them to green.
    harness.reboot_preserving(hs::CAUSE_SOFTWARE_RESET);
    harness.run_until_boot_complete();
    assert_eq!(harness.snapshot().level, RecoveryLevel::Yellow);
    for _ in 0..lp_recovery::tuning::CLEAN_COMPLETIONS_TO_GREEN {
        let (_, result) = inject(&mut harness, hs::FAULT_CLEAN_CHILD, 1);
        assert_eq!(result, hs::FAULT_RESULT_OK);
        let (_, result) = inject(&mut harness, hs::FAULT_CLEAN_CHILD, 2);
        assert_eq!(result, hs::FAULT_RESULT_OK);
    }
    let snapshot = harness.snapshot();
    assert_eq!(
        snapshot.level,
        RecoveryLevel::Green,
        "entries: {:?}",
        entry_states(&snapshot)
    );
}

#[test_log::test]
#[ignore = "slow emulator suite; run via `just test-recovery-emu`"]
fn hard_crash_reboots_with_blame() {
    let mut harness = RecoveryEmuHarness::new();
    harness.boot(hs::CAUSE_POWER_ON, false);
    harness.run_until_boot_complete();

    let (outcome, _) = inject(&mut harness, hs::FAULT_HARD_PANIC, 1);
    assert_eq!(
        outcome,
        RunOutcome::ResetRequested,
        "uncaught panic must request a reset"
    );

    harness.reboot_preserving(hs::CAUSE_SOFTWARE_RESET);
    harness.run_until_boot_complete();
    let snapshot = harness.snapshot();
    assert_eq!(snapshot.level, RecoveryLevel::Yellow);
    let crash = snapshot.last_crash.expect("crash reported after reboot");
    assert_eq!(crash.cause, CrashCause::Panic);
    assert_eq!(crash.boots_ago, 1);
    assert!(
        crash.msg.as_str().contains("injected hard panic"),
        "message: {}",
        crash.msg.as_str()
    );
    let path = crash.path_display().to_string();
    assert!(
        path.contains("fault-parent") && path.contains("fault/a"),
        "path: {path}"
    );
}

#[test_log::test]
#[ignore = "slow emulator suite; run via `just test-recovery-emu`"]
fn hang_is_watchdog_attributed() {
    let mut harness = RecoveryEmuHarness::new();
    harness.boot(hs::CAUSE_POWER_ON, false);
    harness.run_until_boot_complete();

    harness.set_fault(hs::FAULT_HANG, 1);
    let outcome = harness.run_with_fuel(HANG_FUEL);
    assert_eq!(
        outcome,
        RunOutcome::FuelExhausted,
        "hang must exhaust the fuel budget"
    );

    harness.reboot_preserving(hs::CAUSE_WATCHDOG_RESET);
    harness.run_until_boot_complete();
    let snapshot = harness.snapshot();
    assert_eq!(snapshot.level, RecoveryLevel::Yellow);
    let crash = snapshot.last_crash.expect("watchdog crash attributed");
    assert_eq!(crash.cause, CrashCause::Watchdog);
    let path = crash.path_display().to_string();
    assert!(
        path.contains("fault-parent") && path.contains("fault/a"),
        "the leftover frame stack is the blame record; path: {path}"
    );
}

/// Fuel exhaustion end-to-end over the real wire: a shader whose render
/// loops forever must abort **in-frame** with a legible node error and
/// ledger blame — never a reboot (contrast [`hang_is_watchdog_attributed`],
/// where a non-shader hang really does burn the whole budget). A repeat
/// offense red-gates the node (the retry latch of the fuel ADR) while the
/// rest of the project keeps rendering.
///
/// The looping shader publishes on a bus channel nothing consumes, so the
/// only renders are the ones this test triggers via render-product probes —
/// giving exact control over offense counts. The blame ledger is inspected
/// directly in guest RAM (no frames consumed), which matters because every
/// server frame tick is a clean completion on the node's path and three of
/// those would heal a yellow entry.
#[tokio::test]
#[test_log::test]
#[ignore = "slow emulator suite; run via `just test-recovery-emu`"]
async fn fuel_exhausted_shader_gates_without_reboot() {
    let fw_emu_path = ensure_binary_built(
        BinaryBuildConfig::new("fw-emu")
            .with_target("riscv32imac-unknown-none-elf")
            .with_profile("release-emu")
            .with_backtrace_support(true)
            .with_unwind_support(true)
            .with_build_std(true),
    )
    .expect("Failed to build fw-emu");
    let elf_data = std::fs::read(&fw_emu_path).expect("Failed to read fw-emu ELF");
    let load_info = load_elf(&elf_data).expect("Failed to load ELF");
    let area_addr = *load_info
        .symbol_map
        .get(hs::RECOVERY_AREA_SYMBOL)
        .expect("recovery area symbol in fw-emu symbol map");
    let ram_size = load_info.ram.len();
    let mut emulator = Riscv32Emulator::new(load_info.code, load_info.ram)
        .with_log_level(LogLevel::None)
        .with_time_mode(TimeMode::Simulated(0))
        .with_allow_unaligned_access(true);
    let sp_value = RAM_BASE.wrapping_add((ram_size as u32).wrapping_sub(16));
    emulator.set_register(Gpr::Sp, sp_value as i32);
    emulator.set_pc(load_info.entry_point);

    // Power-on boot: explicit reset cause, no pending fault (the cold-boot
    // half of `RecoveryEmuHarness::boot`).
    write_guest_u32(
        &mut emulator,
        area_addr + hs::RESET_CAUSE_OFFSET as u32,
        hs::CAUSE_POWER_ON,
    );
    write_guest_u32(
        &mut emulator,
        area_addr + hs::FAULT_REQUEST_OFFSET as u32,
        0,
    );
    write_guest_u32(&mut emulator, area_addr + hs::FAULT_ARG_OFFSET as u32, 0);
    write_guest_u32(&mut emulator, area_addr + hs::FAULT_RESULT_OFFSET as u32, 0);

    let emulator = Arc::new(Mutex::new(emulator));
    let transport = SerialEmuClientTransport::new(emulator.clone())
        .with_backtrace(load_info.symbol_map.clone(), load_info.code_end);
    let client = TokioLpClient::new(Box::new(transport));

    // Project: a healthy default chain (clock → shader → texture → fixture →
    // output) plus a fuel-hungry shader on an unconsumed channel.
    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());
    builder.clock_basic();
    let texture_path = builder.texture().width(2).height(2).add(&mut builder);
    builder.shader_basic(&texture_path);
    let looping = builder
        .shader(&texture_path)
        .glsl(LOOPING_SHADER_GLSL)
        .visual_bus("visual.gated");
    looping.add(&mut builder);
    let output_path = builder.output_basic();
    builder.fixture_basic(&output_path, &texture_path);
    builder.build();

    let project_dir = "project";
    for (path, content) in collect_project_files(&fs.borrow()) {
        let full_path = format!("/projects/{project_dir}/{path}");
        client
            .fs_write(full_path.as_path(), content)
            .await
            .expect("Failed to write project file");
    }
    let handle = client
        .project_load(project_dir)
        .await
        .expect("Failed to load project");

    let good_shader = read_node_id_for_suffix(&client, handle, "/shader.shader").await;
    let bad_shader = read_node_id_for_suffix(&client, handle, "/shader_2.shader").await;

    // Baseline: healthy chain renders, ledger green.
    advance_guest_time(&emulator, 40);
    let frame_num_before = read_project_frame_num(&client, handle).await;
    assert_texture_probe_ok(probe_render(&client, handle, good_shader).await);
    let snapshot = recovery_snapshot(&emulator, area_addr);
    assert_eq!(snapshot.level, RecoveryLevel::Green);

    // Offense 1: the render aborts in-frame (the probe read completes over
    // the live transport — a hang would exhaust the transport's instruction
    // budget instead) with the legible fuel diagnostic; ledger goes yellow
    // on the node's path.
    advance_guest_time(&emulator, 40);
    let message = probe_render_error(&client, handle, bad_shader).await;
    assert!(
        message.contains(
            "shader fuel exhausted: render_texture pixel (0, 0) exceeded 100000 iterations"
        ),
        "first offense should report the fuel diagnostic; message: {message}"
    );
    let snapshot = recovery_snapshot(&emulator, area_addr);
    assert_eq!(snapshot.level, RecoveryLevel::Yellow);
    assert!(
        entry_states(&snapshot)
            .iter()
            .any(|(name, state)| name.contains("shader_2") && *state == "yellow"),
        "entries: {:?}",
        entry_states(&snapshot)
    );
    assert!(
        snapshot.last_crash.is_none(),
        "a recovered fuel abort must not stage a reboot crash record"
    );

    // Offense 2 (immediately — no wire reads in between, so clean produce
    // ticks cannot heal the yellow first): red-gate.
    advance_guest_time(&emulator, 40);
    let message = probe_render_error(&client, handle, bad_shader).await;
    assert!(
        message.contains("shader fuel exhausted"),
        "second offense message: {message}"
    );
    let snapshot = recovery_snapshot(&emulator, area_addr);
    assert_eq!(snapshot.level, RecoveryLevel::Red);
    assert!(
        entry_states(&snapshot)
            .iter()
            .any(|(name, state)| name.contains("shader_2") && *state == "red"),
        "entries: {:?}",
        entry_states(&snapshot)
    );

    // The node status carries the fuel error (red entries ignore clean
    // completions, so wire reads are safe from here on).
    let status = read_node_status(&client, handle, bad_shader).await;
    let NodeRuntimeStatus::Error(status_message) = status else {
        panic!("expected error status on the looping shader, got: {status:?}");
    };
    assert!(
        status_message.contains("shader fuel exhausted"),
        "status: {status_message}"
    );

    // Offense 3: denied up front — the red gate is the retry latch; the
    // shader body never runs again.
    advance_guest_time(&emulator, 40);
    let message = probe_render_error(&client, handle, bad_shader).await;
    assert!(
        message.contains("recovery:"),
        "gated render should be denied up front; message: {message}"
    );

    // The rest of the project is untouched: healthy shader still renders,
    // frames keep advancing, and the device never rebooted.
    advance_guest_time(&emulator, 40);
    assert_texture_probe_ok(probe_render(&client, handle, good_shader).await);
    let frame_num_after = read_project_frame_num(&client, handle).await;
    assert!(
        frame_num_after > frame_num_before,
        "project frames must keep advancing ({frame_num_before} -> {frame_num_after})"
    );
    let snapshot = recovery_snapshot(&emulator, area_addr);
    assert_eq!(snapshot.boot_count, 1, "no reboot may have happened");
    assert!(!snapshot.safe_mode);
    assert!(
        snapshot.last_crash.is_none(),
        "no watchdog/panic reboot attribution may exist"
    );
}

#[test_log::test]
#[ignore = "slow emulator suite; run via `just test-recovery-emu`"]
fn boot_crash_loop_enters_safe_mode_and_recovers() {
    let mut harness = RecoveryEmuHarness::new();

    // Boot 1: crashes during boot (before the milestone).
    harness.boot(hs::CAUSE_POWER_ON, false);
    harness.set_fault(hs::FAULT_BOOT_PANIC, 0);
    assert_eq!(harness.run_frame(), RunOutcome::ResetRequested);

    // Boot 2: same story.
    harness.reboot_preserving(hs::CAUSE_SOFTWARE_RESET);
    harness.set_fault(hs::FAULT_BOOT_PANIC, 0);
    assert_eq!(harness.run_frame(), RunOutcome::ResetRequested);

    // Boot 3: no fault — and the guest reports safe mode.
    harness.reboot_preserving(hs::CAUSE_SOFTWARE_RESET);
    harness.run_until_boot_complete();
    let snapshot = harness.snapshot();
    assert!(snapshot.safe_mode, "two incomplete boots => safe mode");
    assert_eq!(snapshot.consecutive_incomplete_boots, 2);
    let crash = snapshot.last_crash.expect("boot crash reported");
    assert!(
        crash.msg.as_str().contains("injected boot panic"),
        "message: {}",
        crash.msg.as_str()
    );

    // The completed boot forgives: next boot is normal again.
    harness.reboot_preserving(hs::CAUSE_SOFTWARE_RESET);
    harness.run_until_boot_complete();
    let snapshot = harness.snapshot();
    assert!(!snapshot.safe_mode);
    assert_eq!(snapshot.consecutive_incomplete_boots, 0);
}

// --- helpers for the wire-driven fuel test ---------------------------------

/// Visual shader whose per-pixel render never terminates: every invocation
/// exhausts its fuel tank (`lpvm::DEFAULT_INVOCATION_FUEL` back-edges).
const LOOPING_SHADER_GLSL: &str = "\
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;
vec4 render(vec2 pos) {
    float x = 0.0;
    while (true) {
        x = x + 1.0;
    }
    return vec4(x, 0.0, 0.0, 1.0);
}
";

fn write_guest_u32(emulator: &mut Riscv32Emulator, addr: u32, value: u32) {
    let offset = (addr - RAM_BASE) as usize;
    emulator.memory_mut().ram_mut()[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn advance_guest_time(emulator: &Arc<Mutex<Riscv32Emulator>>, ms: u32) {
    emulator.lock().unwrap().advance_time(ms);
}

/// Read the recovery region straight out of guest RAM — no server frames are
/// driven, so ledger state cannot shift under the assertion.
fn recovery_snapshot(emulator: &Arc<Mutex<Riscv32Emulator>>, area_addr: u32) -> RecoverySnapshot {
    let offset = (area_addr + hs::REGION_OFFSET as u32 - RAM_BASE) as usize;
    let emulator = emulator.lock().unwrap();
    let bytes = &emulator.memory().ram()[offset..offset + RecoveryRegion::SIZE];
    RecoveryRegion::read_from_bytes(bytes)
        .expect("region bytes")
        .inspect()
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

/// Apply a project-read event stream onto a fresh [`ProjectView`].
fn view_from_events(events: Vec<lpc_wire::ProjectReadEvent>) -> ProjectView {
    let mut view = ProjectView::new();
    let mut applier = ProjectReadApplier::new(&mut view);
    let mut completed = false;
    for event in events {
        match applier.apply(event).expect("apply project read event") {
            ApplyStatus::Continue => {}
            ApplyStatus::Complete { .. } => completed = true,
        }
    }
    assert!(completed, "project read stream did not complete");
    view
}

async fn read_nodes_view(
    client: &TokioLpClient,
    handle: lpc_wire::WireProjectHandle,
) -> ProjectView {
    let events = client
        .project_read(
            handle,
            ProjectReadRequest {
                since: None,
                queries: vec![ProjectReadQuery::Nodes(NodeReadQuery {
                    level: ReadLevel::Detail,
                    nodes: Default::default(),
                    include_slots: false,
                })],
                probes: Vec::new(),
            },
        )
        .await
        .expect("Failed to read project nodes");
    view_from_events(events)
}

async fn read_node_id_for_suffix(
    client: &TokioLpClient,
    handle: lpc_wire::WireProjectHandle,
    suffix: &str,
) -> NodeId {
    let view = read_nodes_view(client, handle).await;
    let mut available = Vec::new();
    for (id, entry) in &view.tree.nodes {
        let node_path = entry.path.to_string();
        if node_path.ends_with(suffix) {
            return *id;
        }
        available.push(node_path);
    }
    panic!("node path ending in {suffix} not found; available paths: {available:?}");
}

async fn read_node_status(
    client: &TokioLpClient,
    handle: lpc_wire::WireProjectHandle,
    node: NodeId,
) -> NodeRuntimeStatus {
    let view = read_nodes_view(client, handle).await;
    view.tree
        .nodes
        .get(&node)
        .unwrap_or_else(|| panic!("node {node:?} missing from tree read"))
        .status
        .clone()
}

async fn read_project_frame_num(
    client: &TokioLpClient,
    handle: lpc_wire::WireProjectHandle,
) -> u64 {
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
        .expect("Failed to read project runtime");
    view_from_events(events)
        .runtime
        .as_ref()
        .expect("project read should include runtime status")
        .project
        .frame_num
}

/// Trigger one render of `node` via a render-product probe (4x4 RGBA16).
async fn probe_render(
    client: &TokioLpClient,
    handle: lpc_wire::WireProjectHandle,
    node: NodeId,
) -> RenderProductProbeResult {
    let events = client
        .project_read(
            handle,
            ProjectReadRequest {
                since: None,
                queries: Vec::new(),
                probes: vec![ProjectProbeRequest::RenderProduct(
                    RenderProductProbeRequest {
                        product: lpc_model::VisualProduct::new(node, 0),
                        width: 4,
                        height: 4,
                        format: WireTextureFormat::Rgba16,
                    },
                )],
            },
        )
        .await
        .expect("render probe read should complete without a reboot");
    let mut view = ProjectView::new();
    let mut applier = ProjectReadApplier::new(&mut view);
    let mut probes = Vec::new();
    for event in events {
        if let ApplyStatus::Complete { .. } = applier.apply(event).expect("apply probe read event")
        {
            probes = applier.take_completed_probe_results();
        }
    }
    let probe = probes.into_iter().next().expect("probe result present");
    let ProjectProbeResult::RenderProduct(render) = probe else {
        panic!("expected a render-product probe result, got {probe:?}");
    };
    render
}

async fn probe_render_error(
    client: &TokioLpClient,
    handle: lpc_wire::WireProjectHandle,
    node: NodeId,
) -> String {
    match probe_render(client, handle, node).await {
        RenderProductProbeResult::Error { message, .. } => message,
        other => panic!("expected a probe error, got {other:?}"),
    }
}

fn assert_texture_probe_ok(result: RenderProductProbeResult) {
    match result {
        RenderProductProbeResult::Texture { bytes, .. } => {
            assert!(!bytes.is_empty(), "probe texture should carry bytes");
        }
        other => panic!("expected a texture probe result, got {other:?}"),
    }
}
