//! RGB MSAFluid live demo on the `examples/basic` 241-lamp circular
//! ring fixture, GPIO18. Solver + emitters + sampler + readout +
//! `lpc_shared::DisplayPipeline` + RMT, all in `no_std` Rust.
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
use lpc_shared::{DisplayPipeline, DisplayPipelineOptions};

use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::logger;
use crate::output::LedChannel;
use crate::serial::Esp32UsbSerialIo;
use crate::tests::fluid_demo::emitters::FluidPulser;
use crate::tests::fluid_demo::readout::{FRAME_BYTES, render_frame};
use crate::tests::fluid_demo::ring_geometry::{LAMP_COUNT, build_lamp_positions};
use crate::tests::msafluid_solver::MsaFluidSolver;

// ---- Tunables -------------------------------------------------------

const GRID_N: usize = 20;
const SOLVER_ITERS: usize = 3;
const SOLVER_HZ_TARGET: u64 = 25;
const INTENSITY: f32 = 2.5;
const TARGET_X: f32 = 0.5;
const TARGET_Y: f32 = 0.5;
const BRIGHTNESS: f32 = 0.12;
/// Per-step dye decay (lp2014 `FluidRenderer.fadeSpeed` default = 0.01).
/// Without this, dye accumulates forever and the field saturates to white.
const FADE_SPEED: f32 = 0.1;
/// Velocity-field viscosity (lp2014 `MSAFluidSolver2D` default = 0.0001).
/// Higher = smoother / damped flow; lower = more chaotic, longer-lived eddies.
const VISCOSITY: f32 = 0.00003;

// ---- Entry point ----------------------------------------------------

pub async fn run_fluid_demo(_: embassy_executor::Spawner) -> ! {
    let (sw_int, timg0, rmt_peripheral, usb_device, gpio18, _flash, _gpio4) = init_board();
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

    // RMT + LedChannel on gpio18 (matches `examples/basic` strip output).
    let rmt = Rmt::new(rmt_peripheral, Rate::from_mhz(80)).expect("Failed to initialize RMT");
    let mut led_channel = LedChannel::new(rmt, gpio18, LAMP_COUNT)
        .expect("Failed to initialize LED channel on gpio18");

    // DisplayPipeline (interpolation + LUT + dither).
    let options = DisplayPipelineOptions {
        lum_power: 2.0,
        white_point: [1.0, 1.0, 1.0],
        brightness: BRIGHTNESS,
        interpolation_enabled: true,
        dithering_enabled: false,
        lut_enabled: true,
    };
    let mut pipeline =
        DisplayPipeline::new(LAMP_COUNT as u32, options).expect("Failed to create DisplayPipeline");

    // Solver.
    let mut solver = MsaFluidSolver::new(GRID_N, GRID_N);
    solver.set_solver_iterations(SOLVER_ITERS);
    solver.set_fade_speed(lps_q32::Q32::from_f32_wrapping(FADE_SPEED));
    solver.set_viscosity(lps_q32::Q32::from_f32_wrapping(VISCOSITY));

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

    info!("[fluid_demo] solver_period_us={solver_period_us} (target {SOLVER_HZ_TARGET} Hz)",);

    loop {
        let now_us = start.elapsed().as_micros();

        if now_us.saturating_sub(last_solver_us) >= solver_period_us {
            let now_ms = now_us / 1000;
            pulser.tick(&mut solver, now_ms, TARGET_X, TARGET_Y, INTENSITY);
            solver.update();
            render_frame(&solver, &lamp_positions, &mut rgb_frame);
            // Stamp the frame one period in the future so DisplayPipeline's
            // temporal interpolation has a real prev_ts → current_ts window
            // that brackets `now_us` on subsequent ticks. Without the
            // forward stamp, every tick sees frame_progress_us == delta and
            // renders `current` directly with no lerp.
            let frame_ts_us = now_us.saturating_add(solver_period_us);
            pipeline.write_frame_from_u8(frame_ts_us, &rgb_frame);
            solver_count = solver_count.wrapping_add(1);
            last_solver_us = now_us;
        }

        pipeline.tick(now_us, &mut led_buf);
        let tx = led_channel.start_transmission(&led_buf);
        led_channel = tx.wait_complete();
        display_count = display_count.wrapping_add(1);

        if now_us.saturating_sub(last_log_us) >= 1_000_000 {
            info!("[fluid_demo] solver_hz={solver_count} display_hz={display_count}",);
            solver_count = 0;
            display_count = 0;
            last_log_us = now_us;
        }
    }
}
