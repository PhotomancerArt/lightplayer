//! RMT driver test mode
//!
//! When `test_rmt` feature is enabled, this runs simple LED patterns
//! to verify the RMT driver works correctly.

extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;
use esp_hal::rmt::Rmt;
use esp_hal::time::Rate;

use crate::board::{init_board, start_runtime};
use crate::logger;
use crate::output::{LedChannel, LedTransaction};
use crate::serial::Esp32UsbSerialIo;
use fw_core::serial::SerialIo;

/// Run RMT test mode
///
/// Displays simple patterns on LEDs to verify RMT driver works.
pub async fn run_rmt_test() -> ! {
    // Initialize board (clock, heap, runtime) and get hardware peripherals
    let (sw_int, timg0, rmt_peripheral, usb_device, gpio18) = init_board();
    start_runtime(timg0, sw_int);

    // Initialize USB-serial for logging (synchronous mode)
    let usb_serial = esp_hal::usb_serial_jtag::UsbSerialJtag::new(usb_device);
    let serial_io = Esp32UsbSerialIo::new(usb_serial);
    let serial_io_shared = Rc::new(RefCell::new(serial_io));

    // Initialize logger using static function approach (like main.rs)
    logger::set_log_serial(serial_io_shared.clone());
    logger::init(logger::log_write_bytes);

    // Give USB serial a moment to initialize
    embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;

    info!("RMT test mode starting...");

    // Configure RMT (we already have rmt_peripheral from init_board)
    let rmt = Rmt::new(rmt_peripheral, Rate::from_mhz(80)).expect("Failed to initialize RMT");

    // Use GPIO18 (pin 10 on board) for LED output (hardcoded for testing)
    let pin = gpio18;

    // Initialize RMT driver for 8 LEDs
    const NUM_LEDS: usize = 256;
    let mut channel =
        LedChannel::new(rmt, pin, NUM_LEDS).expect("Failed to initialize LED channel");

    info!("RMT driver initialized (LedChannel created), starting chase pattern...");
    // Using full new API: channel.start_transmission().wait_complete()

    loop {
        // Chase pattern - white dot moving down the strip
        info!("Chase pattern");
        let mut data = [0u8; NUM_LEDS * 3];
        for offset in 0..NUM_LEDS {
            for i in 0..NUM_LEDS {
                if i == offset {
                    data[i * 3] = 10; // R
                    data[i * 3 + 1] = 10; // G
                    data[i * 3 + 2] = 10; // B
                } else {
                    data[i * 3] = 0; // R
                    data[i * 3 + 1] = 0; // G
                    data[i * 3 + 2] = 0; // B
                }
            }
            let tx = channel.start_transmission(&data);
            channel = tx.wait_complete();
            embassy_time::Timer::after(embassy_time::Duration::from_millis(10)).await;
        }
    }
}

/// Convert HSV to RGB
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}
