//! DisplayPipeline test mode
//!
//! When `test_dither` feature is enabled, this runs LED patterns through
//! the full DisplayPipeline (interpolation, dithering, gamma LUT, brightness)
//! to verify the pipeline works correctly.

extern crate alloc;

use alloc::rc::Rc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use esp_hal::rmt::Rmt;
use esp_hal::time::Rate;
use log::info;
use lp_shared::{DisplayPipeline, DisplayPipelineOptions};

use crate::board::{init_board, start_runtime};
use crate::logger;
use crate::output::LedChannel;
use crate::serial::Esp32UsbSerialIo;

/// Run DisplayPipeline test mode
///
/// Sends 16-bit data through the pipeline (interpolation, dithering, LUT, brightness)
/// and outputs to LEDs via RMT.
pub async fn run_dithering_test() -> ! {
    let (sw_int, timg0, rmt_peripheral, usb_device, gpio18) = init_board();
    start_runtime(timg0, sw_int);

    let usb_serial = esp_hal::usb_serial_jtag::UsbSerialJtag::new(usb_device);
    let serial_io = Esp32UsbSerialIo::new(usb_serial);
    let serial_io_shared = Rc::new(RefCell::new(serial_io));

    logger::set_log_serial(serial_io_shared.clone());
    logger::init(logger::log_write_bytes);

    embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;

    info!("DisplayPipeline test mode starting...");

    let rmt = Rmt::new(rmt_peripheral, Rate::from_mhz(80)).expect("Failed to initialize RMT");
    let pin = gpio18;

    const NUM_LEDS: usize = 256;
    let mut channel =
        LedChannel::new(rmt, pin, NUM_LEDS).expect("Failed to initialize LED channel");

    info!("Creating DisplayPipeline with interpolation, dithering, LUT, brightness=1.0");

    let options = DisplayPipelineOptions {
        lum_power: 2.0,
        white_point: [0.9, 1.0, 1.0],
        brightness: 1.0, // Full range; ramp encodes 0-25% in data
        interpolation_enabled: true,
        dithering_enabled: true,
        lut_enabled: true,
    };
    let mut pipeline =
        DisplayPipeline::new(NUM_LEDS as u32, options).expect("Failed to create DisplayPipeline");

    let mut frame_ts_us: u64 = 0;
    const FRAME_INTERVAL_US: u64 = 16_667;
    // 0-25% of 16-bit max
    const RAMP_MAX: u32 = 65535 / 4;
    // Advance 1 pixel every 4 frames for slow rotation
    const PHASE_STEP_EVERY: u32 = 4;

    let mut out_buf = Vec::with_capacity(NUM_LEDS * 3);
    out_buf.resize(NUM_LEDS * 3, 0);

    info!("Starting rotating 0-25% brightness ramp (16-bit -> pipeline -> 8-bit -> RMT)");

    let mut frame_count: u32 = 0;
    loop {
        let phase = ((frame_count / PHASE_STEP_EVERY) as usize) % NUM_LEDS;

        let mut data_16 = vec![0u16; NUM_LEDS * 3];
        for i in 0..NUM_LEDS {
            let ramppos = (i + NUM_LEDS - phase) % NUM_LEDS;
            let val = (ramppos as u32 * RAMP_MAX / NUM_LEDS.max(1) as u32) as u16;
            data_16[i * 3] = val;
            data_16[i * 3 + 1] = val;
            data_16[i * 3 + 2] = val;
        }

        pipeline.write_frame(frame_ts_us, &data_16);
        frame_ts_us = frame_ts_us.saturating_add(FRAME_INTERVAL_US);
        pipeline.write_frame(frame_ts_us, &data_16);

        let tick_time = frame_ts_us.saturating_sub(FRAME_INTERVAL_US / 2);
        pipeline.tick(tick_time, &mut out_buf);

        let tx = channel.start_transmission(&out_buf);
        channel = tx.wait_complete();

        frame_count = frame_count.wrapping_add(1);
        embassy_time::Timer::after(embassy_time::Duration::from_millis(30)).await;
    }
}
