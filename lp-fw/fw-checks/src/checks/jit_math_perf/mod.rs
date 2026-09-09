//! The `jit-math-perf` payload: Q32 fixed-point math kernel cycle costs.
//!
//! Candidate JIT hot-path math kernels (multiply, divide, sine — the corpus
//! `lpvm`'s Q32 `lps-builtins` shader runtime actually calls), measured
//! against a real cycle counter and each other. It answers "how many cycles
//! does this kernel cost", never "does the shader pipeline work" — there is
//! no shader compile or execution here, only the arithmetic and the corpus
//! that exercises it.
//!
//! Q12 retirement ledger (2026-09-06 esp-emulator plan, `notes.md`): the
//! cheapest remaining `test_*` migration, because the whole harness was
//! already `lps-builtins` calls plus cycle timing — no product wire types to
//! keep out, unlike `test_json`'s heartbeat.
//!
//! **The cycle counter is the one thing that stays outside this crate.** The
//! ESP32-C6 has no standard RISC-V Zicntr CSR; reading its PMU counter is a
//! chip fact, not arithmetic over bytes, so it is injected as a plain
//! `fn() -> u32` rather than read here — the firmware hands in
//! `board::esp32c6::cycle_counter::read`. Everything downstream (corpus,
//! kernels, statistics, the JSON record) lives in this crate and does not
//! know which chip it is running on.

pub mod corpus;
mod div_kernels;
mod lut_cost;
mod mul_kernels;
pub mod runner;
mod trig_kernels;

/// The sentinel line. The payload runs to completion, unlike `gpio-calibrate`
/// and `uart-bridge`, which serve forever.
pub const DONE_MARKER: &str = "[jit-math-perf] === DONE ===";

/// Run the whole corpus — overhead baseline, LUT access cost, multiply,
/// divide, trig — and print the sentinel. `read_cycles` is the chip's cycle
/// counter (see the module docs).
pub fn run_all(read_cycles: fn() -> u32) {
    log::info!("[jit-math-perf] === JIT math perf experiment starting ===");
    runner::run_overhead_baseline(read_cycles);
    lut_cost::run(read_cycles);
    mul_kernels::run(read_cycles);
    div_kernels::run(read_cycles);
    trig_kernels::run(read_cycles);
    log::info!("{DONE_MARKER}");
}
