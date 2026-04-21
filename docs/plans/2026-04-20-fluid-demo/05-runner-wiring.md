# Phase 05 — Runner, Cargo feature, main dispatch, justfile

**Tags:** sub-agent: supervised. Depends on phases 1–4.

## Scope of phase

Tie the demo together end-to-end on hardware:

1. Add `test_fluid_demo` feature to `lp-fw/fw-esp32/Cargo.toml`.
2. Write `lp-fw/fw-esp32/src/tests/fluid_demo/runner.rs` —
   `pub async fn run_fluid_demo(_: Spawner) -> !`.
3. Update `mod.rs` to add `pub mod runner;`.
4. Update `main.rs`:
    - Register `mod tests { pub mod msafluid_solver; pub mod fluid_demo; }`
      under `#[cfg(feature = "test_fluid_demo")]`.
    - Add `test_fluid_demo` to every existing `cfg(not(any(...test...)))` /
      `cfg(any(...test...))` set already covering `test_msafluid` etc.
    - Dispatch `run_fluid_demo` from `async fn main`.
5. Add `fwtest-fluid-demo-esp32c6` recipe to `justfile`.
6. (If needed) extend any other module gating in `output/`, `serial/`,
   etc. so the demo builds on its own with the same skeleton as the other
   `test_*` features.

### Out of scope

- Any algorithm changes inside `msafluid_solver.rs`, `emitters.rs`,
  `sampler.rs`, `readout.rs`, or `ring_geometry.rs` beyond the bare
  minimum needed to compile (e.g. exposing a missing accessor — and only
  with permission; if you need to, stop and report).
- Any change to `examples/basic`.
- Tuning the demo (intensity, hz, etc.) past plausible defaults.

## Code organization reminders

- Granular file structure, one concept per file.
- Place abstract things, entry points, and tests near the **top** of
  each file.
- Place helper utility functions at the **bottom** of each file.
- Keep related functionality grouped together.
- Any temporary code must have a `TODO` comment so it can be found
  later.

## Sub-agent reminders

- Do **not** commit. The plan commits at the end as a single unit.
- Do **not** expand scope. Stay strictly within "Scope of phase".
- Do **not** suppress warnings or `#[allow(...)]` problems away — fix
  them.
- Do **not** disable, skip, or weaken existing tests to make the build
  pass.
- If something blocks completion (ambiguity, unexpected design issue),
  stop and report rather than improvising.
- Report back: what changed, what was validated, and any deviations
  from this phase plan.

## Implementation details

### 1. Cargo feature

In `lp-fw/fw-esp32/Cargo.toml`, after the existing `test_msafluid` line,
add:

```toml
test_fluid_demo = []  # RGB MSAFluid demo on the basic ring fixture (gpio4, 241 leds, DisplayPipeline)
```

### 2. `tests/fluid_demo/mod.rs`

After phase 4 it reads:

```rust
pub mod emitters;
pub mod readout;
pub mod ring_geometry;
pub mod sampler;
```

Add:

```rust
pub mod runner;
```

### 3. `tests/fluid_demo/runner.rs`

New file. Mirror the structure of `tests/test_dither.rs` (logger
bring-up, RMT init, `LedChannel::new`) and `tests/test_msafluid.rs`
(solver + Q32 + `Vec`).

```rust
//! RGB MSAFluid live demo on the `examples/basic` 241-lamp circular
//! ring fixture, GPIO4. Solver + emitters + sampler + readout +
//! `lp_shared::DisplayPipeline` + RMT, all in `no_std` Rust.
//!
//! See `docs/plans/2026-04-20-fluid-demo/00-design.md` for the
//! architecture and tunables.

extern crate alloc;

use alloc::rc::Rc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;

use embassy_time::{Duration, Instant, Timer};
use esp_hal::rmt::Rmt;
use esp_hal::time::Rate;
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use log::info;
use lp_shared::{DisplayPipeline, DisplayPipelineOptions};

use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::logger;
use crate::output::LedChannel;
use crate::serial::Esp32UsbSerialIo;
use crate::tests::fluid_demo::emitters::FluidPulser;
use crate::tests::fluid_demo::readout::{render_frame, FRAME_BYTES};
use crate::tests::fluid_demo::ring_geometry::{build_lamp_positions, LAMP_COUNT};
use crate::tests::msafluid_solver::MsaFluidSolver;

// ---- Tunables -------------------------------------------------------

const GRID_N: usize = 32;
const SOLVER_ITERS: usize = 4;
const SOLVER_HZ_TARGET: u64 = 15;
const INTENSITY: f32 = 2.5;
const TARGET_X: f32 = 0.5;
const TARGET_Y: f32 = 0.5;
const BRIGHTNESS: f32 = 0.12;

// ---- Entry point ----------------------------------------------------

pub async fn run_fluid_demo(_: embassy_executor::Spawner) -> ! {
    let (sw_int, timg0, rmt_peripheral, usb_device, _gpio18, _flash, gpio4) =
        init_board();
    start_runtime(timg0, sw_int);

    let usb_serial = UsbSerialJtag::new(usb_device);
    let serial_io = Esp32UsbSerialIo::new(usb_serial);
    let serial_io_shared = Rc::new(RefCell::new(serial_io));
    logger::set_log_serial(serial_io_shared.clone());
    logger::init(logger::log_write_bytes);
    Timer::after(Duration::from_millis(100)).await;

    info!("[fluid_demo] === RGB MSAFluid demo ===");
    info!(
        "[fluid_demo] N={GRID_N} iters={SOLVER_ITERS} solver_hz_target={SOLVER_HZ_TARGET} \
         intensity={INTENSITY} brightness={BRIGHTNESS} lamps={LAMP_COUNT}",
    );

    // RMT + LedChannel on gpio4.
    let rmt = Rmt::new(rmt_peripheral, Rate::from_mhz(80))
        .expect("Failed to initialize RMT");
    let mut led_channel = LedChannel::new(rmt, gpio4, LAMP_COUNT)
        .expect("Failed to initialize LED channel on gpio4");

    // DisplayPipeline (interpolation + LUT + dither).
    let options = DisplayPipelineOptions {
        lum_power: 2.0,
        white_point: [1.0, 1.0, 1.0],
        brightness: BRIGHTNESS,
        interpolation_enabled: true,
        dithering_enabled: true,
        lut_enabled: true,
    };
    let mut pipeline = DisplayPipeline::new(LAMP_COUNT as u32, options)
        .expect("Failed to create DisplayPipeline");

    // Solver.
    let mut solver = MsaFluidSolver::new(GRID_N, GRID_N);
    solver.set_solver_iterations(SOLVER_ITERS);

    // Emitter + geometry.
    let mut pulser = FluidPulser {
        config: Default::default(),
    };
    let lamp_positions = build_lamp_positions();

    // Frame scratch buffers.
    let mut rgb_frame = [0u8; FRAME_BYTES];
    let mut led_buf: Vec<u8> = vec![0u8; LAMP_COUNT * 3];

    let solver_period_us: u64 = 1_000_000 / SOLVER_HZ_TARGET;
    let start = Instant::now();
    let mut last_solver_us: u64 = 0;
    let mut last_log_us: u64 = 0;
    let mut solver_count: u32 = 0;
    let mut display_count: u32 = 0;

    info!(
        "[fluid_demo] solver_period_us={solver_period_us} (target {SOLVER_HZ_TARGET} Hz)",
    );

    loop {
        let now_us = Instant::now().duration_since(start).as_micros();

        if now_us.saturating_sub(last_solver_us) >= solver_period_us {
            let now_ms = now_us / 1000;
            pulser.tick(&mut solver, now_ms, TARGET_X, TARGET_Y, INTENSITY);
            solver.update();
            render_frame(&solver, &lamp_positions, &mut rgb_frame);
            pipeline.write_frame_from_u8(now_us, &rgb_frame);
            solver_count = solver_count.wrapping_add(1);
            last_solver_us = now_us;
        }

        pipeline.tick(now_us, &mut led_buf);
        let tx = led_channel.start_transmission(&led_buf);
        led_channel = tx.wait_complete();
        display_count = display_count.wrapping_add(1);

        if now_us.saturating_sub(last_log_us) >= 1_000_000 {
            info!(
                "[fluid_demo] solver_hz={solver_count} display_hz={display_count}",
            );
            solver_count = 0;
            display_count = 0;
            last_log_us = now_us;
        }
    }
}
```

If `Instant::now().duration_since(start).as_micros()` returns an `i64`
or `u128` on this version of `embassy-time`, cast appropriately —
match what `test_dither.rs` and `test_msafluid.rs` already do.

### 4. `main.rs` updates

Pattern: every place `test_msafluid` appears, add `test_fluid_demo`
alongside it (same `cfg` shape).

Specifically:

a. Top-of-file `#[cfg(not(any(... test_msafluid)))]` for `mod boot`,
   `mod output` (within the inner `not(any(...))`), `mod server_loop`,
   `mod time`, `mod transport`, `mod flash_storage`, `mod lp_fs_flash`,
   and the big `use { ... }` import block — append `feature = "test_fluid_demo",`
   to the `any(...)` list everywhere.

b. The `mod output` outer `#[cfg(any(not(any(...)), feature = "test_*"))]`
   already includes the active test features that need `output`. Add
   `feature = "test_fluid_demo",` to **the inner `not(any(...))`** (so
   the demo path counts as "this is a test build, don't enable
   server-mode default") **and** to the **outer `any(...)`** list (so
   `mod output` is still compiled when only `test_fluid_demo` is on, since
   the demo uses `LedChannel`).

c. Add a new module-registration block:

   ```rust
   #[cfg(feature = "test_fluid_demo")]
   mod tests {
       pub mod fluid_demo;
       pub mod msafluid_solver;
   }
   ```

   This sits next to the existing `#[cfg(feature = "test_msafluid")] mod tests { ... }`
   block.

d. Inside `async fn main`, add a new dispatch arm:

   ```rust
   #[cfg(feature = "test_fluid_demo")]
   {
       use tests::fluid_demo::runner::run_fluid_demo;
       run_fluid_demo(spawner).await;
   }
   ```

   Place it next to the existing `#[cfg(feature = "test_msafluid")]` arm.

e. The big terminal `#[cfg(not(any(... test_msafluid)))]` block at the
   bottom of `async fn main` (the "real server boot" branch): append
   `feature = "test_fluid_demo"` to its `any(...)` list.

f. The standalone `fn esp32_memory_stats` is also gated
   `#[cfg(not(any(...test_msafluid)))]` — append `test_fluid_demo` there
   too.

### 5. justfile

After the `fwtest-msafluid-esp32c6` recipe, add:

```make
# Run firmware with test_fluid_demo: live RGB MSAFluid demo on examples/basic ring fixture (GPIO4)
fwtest-fluid-demo-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32 && cargo run --features test_fluid_demo,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32_profile }}
```

### 6. Things that may surface — handle within scope

- **Missing `Default` for `PulserConfig`.** Already provided in phase 3.
- **`render_frame` signature mismatch.** If phase 4 used a different
  signature, adapt the call site here, *not* the function. If signatures
  diverge in spirit (e.g. `render_frame` doesn't take lamp positions),
  stop and report.
- **`LedChannel::new` pin generic.** It accepts `O: PeripheralOutput`;
  passing `gpio4` works the same way `gpio18` does in `test_dither`.
- **`DisplayPipelineOptions` field shape changed.** Mirror the field
  list used in `test_dither.rs` exactly, just with our values.
- **Heap pressure.** N=32 grid → ~6 KB per `Q32` field × 8 fields =
  ~48 KB. Plus pipeline triple-buffer (~6 × 723 = ~4.3 KB). Should
  comfortably fit in the existing heap config; if `esp-alloc` panics on
  init, stop and report rather than reconfiguring memory.

## Validate

From `lp-fw/fw-esp32/`:

```sh
cargo clippy --features test_fluid_demo,esp32c6 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    -- --no-deps -D warnings
```

Then make sure pre-existing combos still build:

```sh
cargo clippy --features esp32c6 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    -- --no-deps -D warnings
```

```sh
cargo clippy --features test_msafluid,esp32c6 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    -- --no-deps -D warnings
```

```sh
cargo clippy --features test_dither,esp32c6 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    -- --no-deps -D warnings
```

All four must pass clean.

**Do not flash to hardware in this phase.** The user will do that
manually after reviewing the diff.
