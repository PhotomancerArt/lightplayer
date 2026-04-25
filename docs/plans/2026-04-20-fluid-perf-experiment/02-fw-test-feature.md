# Phase 02 — `test_msafluid` firmware feature, multi-resolution cycle measurement

## Sub-agent: yes (Composer 2). Do not commit.

## Scope

Wire the solver from phase 01 into the existing `test_*` Cargo
feature pattern in `fw-esp32`. Add a `test_msafluid` feature that:

- Boots the board (init only — no LEDs, no USB JTAG transport
  needed beyond what `esp_println` already uses for stdout).
- For each resolution N ∈ {16, 32, 48, 64}, allocates a solver,
  injects a force / color impulse at the center on every step,
  measures `mcycle` cycles per `update()` over 30 steps (drop first
  5 as warmup, keep 25), prints median / min / max / avg.
- After all four resolutions, prints a summary table including the
  per-frame budget at 30 fps (5,333,333 cycles) and the percentage
  each resolution consumes.

This phase does **not** implement any LED output or visualization.
The solver runs, we read its cycle cost, we print, we move on.

### Out of scope

- LED strip output. The `r[]` field is computed and discarded.
- Re-running across multiple `dt` / `visc` configurations. One
  config (the lp2014 defaults) per resolution.
- A `loop {}` that repeats the measurement forever. Run once,
  print, then either halt (preferred) or loop a final
  `embassy_time::Timer` so the watchdog stays happy.
- Persisting results to flash / sending to host. The user reads
  serial output and we're done.
- RGB extension. Mono only (matches phase 01).

## Code organization

- New file: `lp-fw/fw-esp32/src/tests/test_msafluid.rs` (mirrors
  `test_dither.rs`).
- New Cargo feature `test_msafluid` in `lp-fw/fw-esp32/Cargo.toml`.
- Wiring in `lp-fw/fw-esp32/src/main.rs`:
  - `#[cfg(feature = "test_msafluid")] mod tests { pub mod test_msafluid; }`
    block alongside the other test mode mod blocks (~line 116).
  - `#[cfg(feature = "test_msafluid")] { ... run ... }` block
    inside `async fn main()` alongside the other test runners
    (~line 159).
  - Add `feature = "test_msafluid"` to the `not(any(...))` guard at
    the bottom (~line 177) so the default firmware path stays gated
    correctly.

## Sub-agent reminders

- Do **not** commit. Phase 03 closes the plan with one commit.
- Do **not** expand scope (no LEDs, no RGB, no fancy harness).
- Do **not** suppress warnings. If the new code generates `unused`
  warnings under default features (because the test runner is gated
  off by default), fix them with proper `#[cfg(feature = ...)]`
  gating, *not* with `#[allow(...)]`.
- Do **not** disable / weaken / skip any existing test runners. Just
  add a new one in the existing pattern.
- If the cycle-counter approach (`mcycle` CSR via `riscv` crate or
  inline asm) doesn't work for some toolchain reason, stop and report
  — do not silently fall back to wall-clock `embassy_time` (resolution
  is too coarse and would invalidate the measurement).
- Report back: files changed, validation output, the *expected*
  serial output format the user will see when they flash, any
  deviations.

## Implementation details

### Cargo.toml

In `lp-fw/fw-esp32/Cargo.toml`, in the `[features]` section, alongside
the existing `test_*` features (~line 24):

```toml
test_msafluid = []  # MSAFluid solver perf experiment (no LEDs, prints cycles to log)
```

The solver does not need `server`, `lp-shared`, RMT, USB serial, or
any of the other existing test_dither dependencies — just the board
init and `esp_println` for stdout. Keep the feature additive-empty.

If `riscv` isn't already a transitive dependency suitable for
direct `use riscv::register::mcycle::read64()`, prefer the inline-asm
approach below (no new dep needed).

### main.rs wiring

Three small edits to `lp-fw/fw-esp32/src/main.rs`. Mirror exactly the
pattern of `test_dither`:

1. Module include block (alongside the others around line 116):

   ```rust
   #[cfg(feature = "test_msafluid")]
   mod tests {
       pub mod msafluid_solver;
       pub mod test_msafluid;
   }
   ```

   Note that `msafluid_solver` (from phase 01) goes here too — it
   only ever needs to be compiled when `test_msafluid` is on. (If
   phase 01 instead added `msafluid_solver` under a different
   `#[cfg(...)]`, reconcile so the module is reachable from
   `test_msafluid` and not built otherwise.)

2. Runner block inside `async fn main()` (alongside the others
   around line 159):

   ```rust
   #[cfg(feature = "test_msafluid")]
   {
       use tests::test_msafluid::run_msafluid_test;
       run_msafluid_test().await;
   }
   ```

3. Update the `not(any(...))` default-firmware guard around line 177
   to include `test_msafluid`:

   ```rust
   #[cfg(not(any(
       feature = "test_rmt",
       feature = "test_dither",
       feature = "test_gpio",
       feature = "test_usb",
       feature = "test_json",
       feature = "test_msafluid",
   )))]
   ```

### test_msafluid.rs structure

```rust
//! MSAFluid solver perf experiment.
//!
//! When `test_msafluid` feature is enabled, this runs the
//! Stam/MSAFluid solver (mono path, Q32) on the esp32c6 at four
//! grid resolutions and prints per-step cycle counts via
//! `esp_println`. No LEDs, no display — just numbers.
//!
//! See docs/plans/2026-04-20-fluid-perf-experiment/00-notes.md for
//! the full context (motivation: theoretical-upper-bound perf data
//! point for fluid on esp32 before committing to the engine
//! pipeline architecture in
//! docs/future/2026-04-20-engine-pipeline-architecture.md).

extern crate alloc;

use lps_q32::Q32;
use log::info;

use crate::board::esp32c6::init::{init_board, start_runtime};
use super::msafluid_solver::MsaFluidSolver;

const RESOLUTIONS: &[usize] = &[16, 32, 48, 64];
const STEPS_TOTAL: usize = 30;
const STEPS_WARMUP: usize = 5;
const ESP32C6_HZ: u64 = 160_000_000;
const TARGET_FPS: u64 = 30;

pub async fn run_msafluid_test() -> ! {
    let (sw_int, timg0, _rmt, _usb, _gpio18) = init_board();
    start_runtime(timg0, sw_int);

    embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;
    info!("[msafluid] === MSAFluid perf experiment starting ===");
    info!(
        "[msafluid] esp32c6 @ {} Hz, frame budget @ {} fps = {} cycles",
        ESP32C6_HZ,
        TARGET_FPS,
        ESP32C6_HZ / TARGET_FPS,
    );

    let mut results: [(usize, u64, u64, u64, u64); 4] = [(0, 0, 0, 0, 0); 4];

    for (slot, &n) in RESOLUTIONS.iter().enumerate() {
        info!("[msafluid] --- N={} ---", n);
        let measurements = run_one(n);
        let avg = measurements.iter().sum::<u64>() / (measurements.len() as u64);
        let mut sorted = measurements.clone();
        sorted.sort_unstable();
        let median = sorted[sorted.len() / 2];
        let min = *sorted.first().unwrap();
        let max = *sorted.last().unwrap();
        let pct_of_budget =
            (median * 1000 * TARGET_FPS) / ESP32C6_HZ; // tenths of a percent
        info!(
            "[msafluid] N={} steps={} (warmup={}) avg={} median={} min={} max={} ({}.{}% of {}fps budget)",
            n,
            STEPS_TOTAL - STEPS_WARMUP,
            STEPS_WARMUP,
            avg,
            median,
            min,
            max,
            pct_of_budget / 10,
            pct_of_budget % 10,
            TARGET_FPS,
        );
        results[slot] = (n, avg, median, min, max);
    }

    info!("[msafluid] === SUMMARY ===");
    info!(
        "[msafluid] esp32c6 @ 160 MHz, frame budget @ {} fps = {} cycles",
        TARGET_FPS,
        ESP32C6_HZ / TARGET_FPS,
    );
    for &(n, avg, median, min, max) in &results {
        let pct = (median * 1000 * TARGET_FPS) / ESP32C6_HZ;
        info!(
            "[msafluid]   N={:2}: median={:>10} cycles ({}.{}% of budget) avg={} min={} max={}",
            n, median, pct / 10, pct % 10, avg, min, max,
        );
    }
    info!("[msafluid] === DONE ===");

    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(60)).await;
    }
}

fn run_one(n: usize) -> alloc::vec::Vec<u64> {
    let mut solver = MsaFluidSolver::new(n, n);

    // Inject a constant force + color impulse at the center each step.
    let center_i = n / 2;
    let center_j = n / 2;
    let force_x = Q32::from_f32_wrapping(0.5);
    let force_y = Q32::from_f32_wrapping(0.3);
    let dye = Q32::from_f32_wrapping(1.0);

    let mut measurements = alloc::vec::Vec::with_capacity(STEPS_TOTAL);
    for step in 0..STEPS_TOTAL {
        solver.add_force_at_cell(center_i, center_j, force_x, force_y);
        solver.add_color_at_cell(center_i, center_j, dye);

        let start = read_mcycle();
        solver.update();
        let end = read_mcycle();
        let cycles = end.wrapping_sub(start);

        if step >= STEPS_WARMUP {
            measurements.push(cycles);
        }
    }
    measurements
}

#[inline(always)]
fn read_mcycle() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!(
            "csrr {lo}, mcycle",
            "csrr {hi}, mcycleh",
            lo = out(reg) lo,
            hi = out(reg) hi,
        );
    }
    ((hi as u64) << 32) | (lo as u64)
}
```

(Adjust the `init_board()` destructuring to whatever `init_board()`
actually returns — the example in this doc may be stale; check
`lp-fw/fw-esp32/src/board/esp32c6/init.rs`. Just make sure the
unused peripherals are bound to `_`.)

### Why `info!` and not `esp_println::println!`

Because the existing `test_*` runners use `info!` via the `log`
facade (see `test_dither.rs`). Stay consistent.

### Heap budget sanity

Worst case (N=64): six `Vec<Q32>` of 66×66 = 4356 elements × 4 bytes
≈ 104 KB. esp32c6 has 512 KB SRAM and the existing `esp_alloc::HEAP`
size in this firmware should accommodate. If the build links but
the device panics with OOM at runtime, that's a real result and the
sub-agent should report it (we'd then know N=64 is heap-bound, not
cycle-bound). **Do not** preemptively shrink the heap or swap
allocators to make it work.

If the existing heap is sized too small (look at `init_board()` /
`esp_alloc::heap_allocator!` macro invocation), increase it just for
this experiment to ~150 KB and note the change in the report.

### Validation

```bash
cargo build -p fw-esp32 --target riscv32imac-unknown-none-elf --release \
    --features test_msafluid
cargo build -p fw-esp32 --target riscv32imac-unknown-none-elf --release
```

Both must succeed. The first turns on the experiment; the second
verifies the default firmware build is unaffected.

Run any project-level convention command if it exists (e.g.
`just build-fw`, `just check`) — check `Justfile` and
`lp-fw/Justfile` first.

`cargo clippy -p fw-esp32 --features test_msafluid --target riscv32imac-unknown-none-elf -- -D warnings`
(or whatever clippy invocation works for the firmware target).

## Report back

- Files changed: list with one-line description each.
- Cargo.toml changes (the new feature line, any new dep lines).
- Validation: exact commands and results for both feature-on and
  feature-off builds.
- The *expected* serial-output format (paste an example with
  placeholder numbers) so the user knows what to look for when they
  flash.
- Any deviations or surprises (e.g. `init_board` signature
  mismatch, heap size adjustment, peripheral binding).
- Whether `riscv` crate ended up as a new dep, or whether inline
  asm was used (and why).
