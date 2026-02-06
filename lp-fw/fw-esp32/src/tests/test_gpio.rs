//! GPIO test mode
//!
//! When `test_gpio` feature is enabled, this cycles through configured GPIO pins,
//! toggling each in a tight loop for 2 seconds to help identify pin numbers.
//!
//! Configure which pins to test by modifying the `GPIO_PINS_TO_TEST` array below.
//! Pin 12 is excluded as it crashes the device.

extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;
use embassy_time::{Duration, Instant};
use esp_hal::gpio::Level;
#[macro_use]
extern crate log;

use crate::board::{init_board, start_runtime};
use crate::logger;
use crate::serial::Esp32UsbSerialIo;

/// GPIO pins to test (exclude pin12 as it crashes the device)
/// Modify this array to change which pins are tested
const GPIO_PINS_TO_TEST: &[u8] = &[
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 14, 15, 16, 17, 18, 19, 20, 21,
];

/// Initialize a GPIO pin as output
fn init_gpio(num: u8, periph: esp_hal::peripherals::GPIO) -> esp_hal::gpio::Output<'static> {
    info!("Initializing GPIO{}...", num);
    esp_hal::gpio::Output::new(periph, Level::Low, esp_hal::gpio::OutputConfig::default())
}

/// Test a GPIO pin by toggling it rapidly for 100ms
fn test_gpio(num: u8, pin: &mut esp_hal::gpio::Output<'static>) {
    info!("Testing GPIO{}...", num);
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
pub async fn run_gpio_test() -> ! {
    // Initialize board (clock, heap, runtime) and get hardware peripherals
    let (sw_int, timg0, _rmt_peripheral, usb_device, _gpio18) = init_board();
    start_runtime(timg0, sw_int);

    // Initialize USB-serial for logging
    let usb_serial = esp_hal::usb_serial_jtag::UsbSerialJtag::new(usb_device);
    let usb_serial_async = usb_serial.into_async();
    let serial_io = Esp32UsbSerialIo::new(usb_serial_async);
    let serial_io_shared = Rc::new(RefCell::new(serial_io));

    // Initialize logger
    let serial_io_for_log = serial_io_shared.clone();
    let write_fn: logger::LogWriteFn = move |msg: &str| {
        if let Ok(mut io) = serial_io_for_log.try_borrow_mut() {
            let _ = io.write(msg.as_bytes());
        }
    };
    logger::init(write_fn);

    // Give USB serial a moment to initialize
    embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;

    info!("GPIO test mode starting...");
    info!("Testing GPIO pins: {:?}", GPIO_PINS_TO_TEST);
    info!("(Pin 12 excluded as it crashes the device)");

    // Get peripherals
    let peripherals = esp_hal::peripherals::Peripherals::take();

    // Initialize all GPIO pins upfront (except GPIO12 which crashes)
    // To change which pins are tested, modify GPIO_PINS_TO_TEST array above
    let mut gpio0 = init_gpio(0, peripherals.GPIO0);
    let mut gpio1 = init_gpio(1, peripherals.GPIO1);
    let mut gpio2 = init_gpio(2, peripherals.GPIO2);
    let mut gpio3 = init_gpio(3, peripherals.GPIO3);
    let mut gpio4 = init_gpio(4, peripherals.GPIO4);
    let mut gpio5 = init_gpio(5, peripherals.GPIO5);
    let mut gpio6 = init_gpio(6, peripherals.GPIO6);
    let mut gpio7 = init_gpio(7, peripherals.GPIO7);
    let mut gpio8 = init_gpio(8, peripherals.GPIO8);
    let mut gpio9 = init_gpio(9, peripherals.GPIO9);
    let mut gpio10 = init_gpio(10, peripherals.GPIO10);
    let mut gpio11 = init_gpio(11, peripherals.GPIO11);
    // GPIO12 excluded - crashes device
    let mut gpio14 = init_gpio(14, peripherals.GPIO14);
    let mut gpio15 = init_gpio(15, peripherals.GPIO15);
    let mut gpio16 = init_gpio(16, peripherals.GPIO16);
    let mut gpio17 = init_gpio(17, peripherals.GPIO17);
    let mut gpio18 = init_gpio(18, peripherals.GPIO18);
    let mut gpio19 = init_gpio(19, peripherals.GPIO19);
    let mut gpio20 = init_gpio(20, peripherals.GPIO20);
    let mut gpio21 = init_gpio(21, peripherals.GPIO21);

    info!("Initialized {} GPIO pins", GPIO_PINS_TO_TEST.len());

    // Test GPIO pins based on GPIO_PINS_TO_TEST array
    loop {
        for &pin_num in GPIO_PINS_TO_TEST {
            match pin_num {
                0 => test_gpio(0, &mut gpio0),
                1 => test_gpio(1, &mut gpio1),
                2 => test_gpio(2, &mut gpio2),
                3 => test_gpio(3, &mut gpio3),
                4 => test_gpio(4, &mut gpio4),
                5 => test_gpio(5, &mut gpio5),
                6 => test_gpio(6, &mut gpio6),
                7 => test_gpio(7, &mut gpio7),
                8 => test_gpio(8, &mut gpio8),
                9 => test_gpio(9, &mut gpio9),
                10 => test_gpio(10, &mut gpio10),
                11 => test_gpio(11, &mut gpio11),
                12 | 13 => {
                    info!("Skipping GPIO12 (crashes device)");
                }
                14 => test_gpio(14, &mut gpio14),
                15 => test_gpio(15, &mut gpio15),
                16 => test_gpio(16, &mut gpio16),
                17 => test_gpio(17, &mut gpio17),
                18 => test_gpio(18, &mut gpio18),
                19 => test_gpio(19, &mut gpio19),
                20 => test_gpio(20, &mut gpio20),
                21 => test_gpio(21, &mut gpio21),
                _ => {
                    info!("Warning: GPIO{} not supported, skipping", pin_num);
                }
            }
        }

        info!("Cycle complete, restarting...");
    }
}
