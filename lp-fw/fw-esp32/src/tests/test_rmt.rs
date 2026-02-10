//! RMT driver test mode
//!
//! When `test_rmt` feature is enabled, this runs simple LED patterns
//! to verify the RMT driver works correctly. Direct 8-bit output, no pipeline.

extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;
use esp_hal::rmt::Rmt;
use esp_hal::time::Rate;
use log::info;

use crate::board::{init_board, start_runtime};
use crate::logger;
use crate::output::LedChannel;
use crate::serial::Esp32UsbSerialIo;

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

    // Initialize RMT driver for LEDs
    const NUM_LEDS: usize = 256;
    let mut channel =
        LedChannel::new(rmt, pin, NUM_LEDS).expect("Failed to initialize LED channel");

    info!("RMT driver initialized (LedChannel created), starting chase pattern...");

    loop {
        // Chase pattern - white dot moving down the strip (direct 8-bit, no pipeline)
        let mut data = [0u8; NUM_LEDS * 3];
        for offset in 0..NUM_LEDS {
            for i in 0..NUM_LEDS {
                if i == offset {
                    data[i * 3] = 10; // R
                    data[i * 3 + 1] = 10; // G
                    data[i * 3 + 2] = 10; // B
                } else {
                    data[i * 3] = 0;
                    data[i * 3 + 1] = 0;
                    data[i * 3 + 2] = 0;
                }
            }
            let tx = channel.start_transmission(&data);
            channel = tx.wait_complete();
            embassy_time::Timer::after(embassy_time::Duration::from_millis(10)).await;
        }
    }
}
