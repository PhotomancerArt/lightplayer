//! The lp-emu-core integration surface added at monorepo landing: instruction
//! and cycle counters (`CycleModel`/`InstClass`), the `LogLevel`-gated
//! instruction ring log, and the debug dumps its consumers read.
//!
//! Payload bytes reuse the conformance corpus (objdump-derived; see
//! tests/conformance.rs).

use lp_xt_emu::{CycleModel, Emulator, LogLevel, RunOutcome};

/// GV1 `spike_stub42`: entry a1,32; movi a2,42; retw. f(_) = 42. (3 insts)
const STUB42: &[u8] = &[0x36, 0x41, 0x00, 0x22, 0xa0, 0x2a, 0x90, 0x00, 0x00];
/// sumloop: backward `j` loop summing 1..=a. f(a) = a*(a+1)/2.
const SUMLOOP: &[u8] = &[
    0x36, 0x41, 0x00, 0x32, 0xa0, 0x00, 0x20, 0x42, 0x20, 0x16, 0x84, 0x00, 0x40, 0x33, 0x80, 0x42,
    0xc4, 0xff, 0xc6, 0xfc, 0xff, 0x30, 0x23, 0x20, 0x90, 0x00, 0x00,
];
/// entry a1,32; ill — raises IllegalInstruction.
const CRASH_ILL: &[u8] = &[0x36, 0x41, 0x00, 0x00, 0x00, 0x00];

#[test]
fn instruction_count_is_exact_for_a_straight_line_run() {
    let mut emu = Emulator::new();
    assert_eq!(emu.run(STUB42, 0, 0), RunOutcome::Ok(42));
    // entry + movi + retw = 3 retired instructions.
    assert_eq!(emu.get_instruction_count(), 3);
    // Default model is InstructionCount: cycles == instructions.
    assert_eq!(emu.get_cycle_count(), 3);
}

#[test]
fn counters_reset_between_runs() {
    let mut emu = Emulator::new();
    assert_eq!(emu.run(SUMLOOP, 0, 10), RunOutcome::Ok(55));
    let long = emu.get_instruction_count();
    assert!(long > 3, "loop run should retire many instructions: {long}");
    assert_eq!(emu.run(STUB42, 0, 0), RunOutcome::Ok(42));
    assert_eq!(
        emu.get_instruction_count(),
        3,
        "counters must reset per run"
    );
}

#[test]
fn cycle_model_weights_apply_per_class() {
    // Same program under both models: InstructionCount charges 1 per
    // instruction; the Esp32C6 table charges >1 for taken branches/jumps, so
    // the loop must cost strictly more cycles under it. (The C6 table is not
    // an Xtensa claim — this pins that the model plumbing actually applies
    // per-class weights.)
    let mut a = Emulator::new().with_cycle_model(CycleModel::InstructionCount);
    assert_eq!(a.run(SUMLOOP, 0, 10), RunOutcome::Ok(55));
    assert_eq!(a.get_cycle_count(), a.get_instruction_count());

    let mut b = Emulator::new().with_cycle_model(CycleModel::Esp32C6);
    assert_eq!(b.run(SUMLOOP, 0, 10), RunOutcome::Ok(55));
    assert_eq!(b.get_instruction_count(), a.get_instruction_count());
    assert!(
        b.get_cycle_count() > b.get_instruction_count(),
        "weighted model should exceed 1 cycle/inst on a branchy loop: {} vs {}",
        b.get_cycle_count(),
        b.get_instruction_count()
    );
}

#[test]
fn instruction_log_fills_only_at_instructions_level() {
    let mut emu = Emulator::new();
    assert_eq!(emu.run(STUB42, 0, 0), RunOutcome::Ok(42));
    assert!(
        emu.format_debug_info(None, 50).contains("log empty"),
        "LogLevel::None must not record instructions"
    );

    let mut emu = Emulator::new().with_log_level(LogLevel::Instructions);
    assert_eq!(emu.run(STUB42, 0, 0), RunOutcome::Ok(42));
    let dump = emu.format_debug_info(None, 50);
    assert!(
        dump.contains("entry"),
        "log should disassemble entry: {dump}"
    );
    assert!(dump.contains("movi"), "log should disassemble movi: {dump}");
    assert!(dump.contains("retw"), "log should disassemble retw: {dump}");
}

#[test]
fn trapping_instruction_is_the_last_log_line() {
    let mut emu = Emulator::new().with_log_level(LogLevel::Instructions);
    let out = emu.run(CRASH_ILL, 0, 0);
    assert!(matches!(out, RunOutcome::Trap(_)), "ill must trap: {out:?}");
    let dump = emu.format_debug_info(Some(emu.cpu.pc), 10);
    let last = dump.lines().last().unwrap_or("");
    assert!(
        last.contains("ill"),
        "trapping instruction should end the log: {dump}"
    );
    assert!(
        last.starts_with('>'),
        "highlight marker should tag the trap pc: {last:?}"
    );
}

#[test]
fn dump_state_reports_registers_and_counters() {
    let mut emu = Emulator::new();
    assert_eq!(emu.run(STUB42, 0, 7), RunOutcome::Ok(42));
    let dump = emu.dump_state();
    assert!(dump.contains("pc="), "state dump has pc: {dump}");
    assert!(
        dump.contains("a0 ..a3"),
        "state dump has register rows: {dump}"
    );
    assert!(
        dump.contains("instructions=3"),
        "state dump has counters: {dump}"
    );
}
