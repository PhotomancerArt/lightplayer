//! The level-4 high-priority-interrupt RMT refill — **the HLI experiment**.
//!
//! An alternative to [`super::shared_driver`]: the same portable refill
//! algorithm, but serviced from a hand-written Xtensa **level-4** vector
//! instead of esp-hal's level-3 dispatch, so a refill can preempt every
//! `rsil ≤ 3` masked window (embassy/`PriorityLock` critical sections, other
//! level ≤ 3 handlers, the level-3 dispatch itself). It cannot preempt
//! `esp-sync`'s `rsil 5` critical sections — `critical_section::with`,
//! esp-radio's `wifi_int_disable`, esp-storage's flash windows — and that
//! boundary is precisely what the experiment's stress matrix measures.
//!
//! Three pieces:
//!
//! * [`vector`] — the assembly: `__naked_level_4_interrupt` (xtensa-lx-rt's
//!   `PROVIDE` override seam), servicing every RMT cause at level 4 against
//!   pre-staged state, plus the interrupt-matrix routing to CPU interrupt 24.
//! * [`app`] — the thread side: a `shared_driver`-shaped surface (configure /
//!   send / abort / telemetry) whose start path runs the host-tested
//!   reference model from `lp-ws281x-hli`.
//! * `lp-ws281x-hli` (crate) — the shared `repr(C)` state contract and the
//!   model itself, host-tested against `lp-ws281x` as oracle.
//!
//! Behind `hli_refill` (endpoint driver swaps this in) and `hli_stress` (the
//! radio-linked head-to-head harness); the shipping image compiles neither.
//! Plan: `2026-08-01-1459-rmt-priority-hli` (P5 reopened as an experiment);
//! ADR: `docs/adr/2026-08-02-classic-hli-refill.md`.

pub mod app;
pub mod vector;
