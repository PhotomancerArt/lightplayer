//! GPIO test mode
//!
//! When `test_gpio` feature is enabled, this cycles through configured GPIO pins,
//! toggling each in a tight loop for 2 seconds to help identify pin numbers.
//!
//! Configure which pins to test by modifying the `GPIO_PINS_TO_TEST` array below.
//! Pin 12 is excluded as it crashes the device.

extern crate alloc;

use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;
use embassy_time::{Duration, Instant};
use esp_hal::gpio::{AnyPin, Level};

use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::logger;
use crate::serial::Esp32UsbSerialIo;

/// GPIO pins to test.
///
/// GPIO12 and GPIO13 are deliberately absent: on the ESP32-C6 they are
/// USB_D- and USB_D+. Reconfiguring either as a GPIO output tears down the
/// USB-Serial-JTAG link this harness logs over — the port disappears from the
/// host mid-run and the board has to be recovered through the BOOT button.
/// Verified on hardware 2026-07-28.
///
/// Modify this array to change which pins are tested.
const GPIO_PINS_TO_TEST: &[u8] = &[
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 14, 15, 16, 17, 18, 19, 20, 21,
];

/// Initialize a GPIO pin as output
///
/// # Safety
/// The caller must guarantee that no other owner holds this GPIO. Board init
/// hands back GPIO4/GPIO18/GPIO20, which the harness drops before any pin is
/// opened here.
unsafe fn init_gpio(num: u8) -> esp_hal::gpio::Output<'static> {
    log::info!("Initializing GPIO{num}...");
    let pin = unsafe { AnyPin::steal(num) };
    esp_hal::gpio::Output::new(pin, Level::Low, esp_hal::gpio::OutputConfig::default())
}

/// Test a GPIO pin by toggling it rapidly for 100ms
fn test_gpio(num: u8, pin: &mut esp_hal::gpio::Output<'static>) {
    log::info!("Testing GPIO{num}...");
    let start_time = Instant::now();
    let mut state = false;
    pin.set_level(Level::High);
    while start_time.elapsed() < Duration::from_millis(100) {
        state = !state;
        if state {
            pin.set_level(Level::High);
        } else {
            pin.set_level(Level::Low);
        }
        // No delay - tight loop for scope visibility
    }
    // Turn off before moving to next pin
    pin.set_level(Level::Low);
}

/// Run GPIO test mode
///
/// Cycles through configured GPIO pins, toggling each in a tight loop for 2 seconds.
/// Prints which GPIO is currently active.
///
/// To change which pins are tested, modify the `GPIO_PINS_TO_TEST` constant above.
/// Pin 12 is excluded as it crashes the device.
pub async fn run_gpio_test(_: embassy_executor::Spawner) -> ! {
    // Initialize board (clock, heap, runtime) and get hardware peripherals
    let (sw_int, timg0, _rmt_peripheral, usb_device, gpio18, _flash, gpio4, gpio20, _wifi, _rwdt) =
        init_board();
    start_runtime(timg0, sw_int);
    // Release the board-owned pins before any pin is stolen below.
    drop(gpio18);
    drop(gpio4);
    drop(gpio20);

    // Initialize USB-serial for logging
    let usb_serial = esp_hal::usb_serial_jtag::UsbSerialJtag::new(usb_device);
    let serial_io = Esp32UsbSerialIo::new(usb_serial);
    let serial_io_shared = Rc::new(RefCell::new(serial_io));

    // Initialize logger. `LogWriteFn` is a plain `fn` pointer, so the serial
    // handle is published to the logger separately instead of being captured.
    logger::set_log_serial(serial_io_shared);
    logger::init(logger::log_write_bytes);

    // Give USB serial a moment to initialize
    embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;

    log::info!("GPIO test mode starting...");
    log::info!("Testing GPIO pins: {GPIO_PINS_TO_TEST:?}");
    log::info!("(GPIO12/GPIO13 excluded: they are USB_D-/USB_D+)");

    // Initialize every pin named by GPIO_PINS_TO_TEST upfront.
    // SAFETY: the board-owned GPIO handles were dropped above, and the two
    // USB data pins are not in the list (see the constant).
    let mut pins: Vec<(u8, esp_hal::gpio::Output<'static>)> = GPIO_PINS_TO_TEST
        .iter()
        .map(|&num| (num, unsafe { init_gpio(num) }))
        .collect();

    log::info!("Initialized {} GPIO pins", pins.len());

    // Test each configured GPIO pin in turn, forever.
    loop {
        for (num, pin) in pins.iter_mut() {
            test_gpio(*num, pin);
        }

        log::info!("Cycle complete, restarting...");
    }
}
