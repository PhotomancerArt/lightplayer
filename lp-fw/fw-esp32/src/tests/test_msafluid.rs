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

use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

use esp_hal::usb_serial_jtag::UsbSerialJtag;
use log::info;
use lps_q32::Q32;

use super::msafluid_solver::MsaFluidSolver;
use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::logger;
use crate::serial::Esp32UsbSerialIo;

const RESOLUTIONS: &[usize] = &[16, 32, 48, 64];
/// Jacobi iteration counts to sweep. lp2014 default is 10; lower values
/// trade convergence for cycles roughly linearly (linear_solver dominates
/// the update — see notes in 00-notes.md).
const ITERATION_COUNTS: &[usize] = &[2, 4, 6, 8, 10];
const STEPS_TOTAL: usize = 30;
const STEPS_WARMUP: usize = 5;
const ESP32C6_HZ: u64 = 160_000_000;
const TARGET_FPS: u64 = 30;

#[derive(Clone, Copy)]
struct RunResult {
    n: usize,
    iters: usize,
    avg: u64,
    median: u64,
    min: u64,
    max: u64,
}

pub async fn run_msafluid_test(_: embassy_executor::Spawner) -> ! {
    let (sw_int, timg0, _rmt, usb_device, _gpio18, _flash, _gpio4) = init_board();
    start_runtime(timg0, sw_int);

    let usb_serial = UsbSerialJtag::new(usb_device);
    let serial_io = Esp32UsbSerialIo::new(usb_serial);
    let serial_io_shared = Rc::new(RefCell::new(serial_io));

    logger::set_log_serial(serial_io_shared.clone());
    logger::init(logger::log_write_bytes);

    embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;
    setup_pmu_cycle_counter();
    info!("[msafluid] === MSAFluid perf experiment starting ===");
    info!(
        "[msafluid] esp32c6 @ {ESP32C6_HZ} Hz, frame budget @ {TARGET_FPS} fps = {} cycles",
        ESP32C6_HZ / TARGET_FPS,
    );
    info!(
        "[msafluid] sweep: N={RESOLUTIONS:?} x iters={ITERATION_COUNTS:?} \
         (steps={STEPS_TOTAL}, warmup={STEPS_WARMUP})",
    );

    let mut results: Vec<RunResult> =
        Vec::with_capacity(RESOLUTIONS.len() * ITERATION_COUNTS.len());

    for &n in RESOLUTIONS {
        info!("[msafluid] --- N={n} ---");
        for &iters in ITERATION_COUNTS {
            let measurements = run_one(n, iters);
            let count = measurements.len() as u64;
            let avg = measurements.iter().sum::<u64>() / count;
            let mut sorted = measurements;
            sorted.sort_unstable();
            let median = sorted[sorted.len() / 2];
            let min = *sorted.first().expect("non-empty after warmup");
            let max = *sorted.last().expect("non-empty after warmup");
            let (pct_int, pct_frac) = pct_of_budget(median);
            info!(
                "[msafluid] N={n:2} iters={iters:2} avg={avg:>10} median={median:>10} \
                 min={min:>10} max={max:>10} ({pct_int:>3}.{pct_frac}% budget)",
            );
            results.push(RunResult {
                n,
                iters,
                avg,
                median,
                min,
                max,
            });
        }
    }

    info!("[msafluid] === SUMMARY ===");
    info!(
        "[msafluid] esp32c6 @ 160 MHz, frame budget @ {TARGET_FPS} fps = {} cycles",
        ESP32C6_HZ / TARGET_FPS,
    );
    for r in &results {
        let (pct_int, pct_frac) = pct_of_budget(r.median);
        info!(
            "[msafluid]   N={n:2} iters={iters:2}: median={median:>10} ({pct_int:>3}.{pct_frac}% budget) \
             avg={avg:>10} min={min:>10} max={max:>10}",
            n = r.n,
            iters = r.iters,
            median = r.median,
            avg = r.avg,
            min = r.min,
            max = r.max,
        );
    }

    info!("[msafluid] === MEDIAN CYCLES MATRIX (rows=N, cols=iters) ===");
    let mut header = alloc::string::String::from("[msafluid]   N \\ iters ");
    for &iters in ITERATION_COUNTS {
        use core::fmt::Write;
        let _ = write!(&mut header, "{iters:>10} ");
    }
    info!("{header}");
    for &n in RESOLUTIONS {
        use core::fmt::Write;
        let mut row = alloc::format!("[msafluid]   N={n:2}         ");
        for &iters in ITERATION_COUNTS {
            let m = results
                .iter()
                .find(|r| r.n == n && r.iters == iters)
                .map(|r| r.median)
                .unwrap_or(0);
            let _ = write!(&mut row, "{m:>10} ");
        }
        info!("{row}");
    }

    info!("[msafluid] === BUDGET MATRIX (% of 30fps frame, rows=N, cols=iters) ===");
    let mut header = alloc::string::String::from("[msafluid]   N \\ iters ");
    for &iters in ITERATION_COUNTS {
        use core::fmt::Write;
        let _ = write!(&mut header, "{iters:>8} ");
    }
    info!("{header}");
    for &n in RESOLUTIONS {
        use core::fmt::Write;
        let mut row = alloc::format!("[msafluid]   N={n:2}         ");
        for &iters in ITERATION_COUNTS {
            let median = results
                .iter()
                .find(|r| r.n == n && r.iters == iters)
                .map(|r| r.median)
                .unwrap_or(0);
            let (pct_int, pct_frac) = pct_of_budget(median);
            let _ = write!(&mut row, "{pct_int:>5}.{pct_frac}% ");
        }
        info!("{row}");
    }

    info!("[msafluid] === DONE ===");

    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(60)).await;
    }
}

fn run_one(n: usize, iters: usize) -> Vec<u64> {
    let mut solver = MsaFluidSolver::new(n, n);
    solver.set_solver_iterations(iters);

    let center_i = n / 2;
    let center_j = n / 2;
    let force_x = Q32::from_f32_wrapping(0.5);
    let force_y = Q32::from_f32_wrapping(0.3);
    let dye = Q32::from_f32_wrapping(1.0);

    let mut measurements = Vec::with_capacity(STEPS_TOTAL - STEPS_WARMUP);
    for step in 0..STEPS_TOTAL {
        solver.add_force_at_cell(center_i, center_j, force_x, force_y);
        // RGB-extended solver: drive only the r channel for the perf test.
        // b/g carry zero work after a few frames of fade, so the per-step
        // cycle measurement remains comparable to pre-RGB baseline data.
        solver.add_color_at_cell(center_i, center_j, dye, Q32::ZERO, Q32::ZERO);

        let start = read_cycle();
        solver.update();
        let end = read_cycle();
        let cycles = end.wrapping_sub(start) as u64;

        if step >= STEPS_WARMUP {
            measurements.push(cycles);
        }
    }
    let _ = (
        solver.r().len(),
        solver.g().len(),
        solver.b().len(),
        solver.nx(),
        solver.ny(),
        solver.stride(),
    );
    measurements
}

/// Returns (integer_percent, tenths_digit) so we can render `XX.Y%` with no
/// floats: e.g. `(45, 1)` for 45.1%.
fn pct_of_budget(median_cycles: u64) -> (u64, u64) {
    let scaled = (median_cycles * 1000 * TARGET_FPS) / ESP32C6_HZ;
    (scaled / 10, scaled % 10)
}

/// Configure the ESP32-C6 PMU to count CPU cycles into `mpccr`.
///
/// The standard RISC-V Zicntr CSRs (`mcycle` 0xB00 and user-mode mirror
/// `cycle` 0xC00) both raise "Illegal instruction" on this part — the
/// LX core only implements Espressif's custom PMU CSRs:
///   * `mpcer`  (0x7E0): event select (1 = cycles)
///   * `mpcmr`  (0x7E1): mode / enable (1 = enabled)
///   * `mpccr`  (0x7E2): 32-bit counter, read with `csrr`
///
/// Counter is 32-bit; at 160 MHz it wraps every ~26.8 s. A single
/// `solver.update()` is far below that, so per-step `wrapping_sub`
/// is correct.
fn setup_pmu_cycle_counter() {
    unsafe {
        core::arch::asm!("csrw 0x7E0, {}", in(reg) 1u32);
        core::arch::asm!("csrw 0x7E1, {}", in(reg) 1u32);
    }
}

#[inline(always)]
fn read_cycle() -> u32 {
    let cycles: u32;
    unsafe {
        core::arch::asm!("csrr {}, 0x7E2", out(reg) cycles);
    }
    cycles
}
