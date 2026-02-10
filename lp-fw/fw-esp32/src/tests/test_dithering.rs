//! DisplayPipeline test mode
//!
//! When `test_dithering` feature is enabled, this runs LED patterns through
//! the full DisplayPipeline (interpolation, dithering, gamma LUT, brightness)
//! to verify the pipeline works correctly.

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use alloc::rc::Rc;
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

    info!("Creating DisplayPipeline with interpolation, dithering, LUT, brightness=0.25");

    let options = DisplayPipelineOptions {
        lum_power: 2.0,
        white_point: [0.9, 1.0, 1.0],
        brightness: 0.25,
        interpolation_enabled: true,
        dithering_enabled: true,
        lut_enabled: true,
    };
    let mut pipeline = DisplayPipeline::new(NUM_LEDS as u32, options)
        .expect("Failed to create DisplayPipeline");

    let mut frame_ts_us: u64 = 0;
    const FRAME_INTERVAL_US: u64 = 16_667;

    let mut out_buf = Vec::with_capacity(NUM_LEDS * 3);
    out_buf.resize(NUM_LEDS * 3, 0);

    info!("Starting chase pattern (16-bit -> pipeline -> 8-bit -> RMT)");

    loop {
        for offset in 0..NUM_LEDS {
            let mut data_16 = vec![0u16; NUM_LEDS * 3];
            for i in 0..NUM_LEDS {
                if i == offset {
                    data_16[i * 3] = 65535;
                    data_16[i * 3 + 1] = 65535;
                    data_16[i * 3 + 2] = 65535;
                }
            }

            pipeline.write_frame(frame_ts_us, &data_16);
            frame_ts_us = frame_ts_us.saturating_add(FRAME_INTERVAL_US);
            pipeline.write_frame(frame_ts_us, &data_16);

            let tick_time = frame_ts_us.saturating_sub(FRAME_INTERVAL_US / 2);
            pipeline.tick(tick_time, &mut out_buf);

            let tx = channel.start_transmission(&out_buf);
            channel = tx.wait_complete();
            embassy_time::Timer::after(embassy_time::Duration::from_millis(10)).await;
        }
    }
}
