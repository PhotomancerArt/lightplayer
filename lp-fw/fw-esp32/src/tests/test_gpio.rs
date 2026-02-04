//! GPIO test mode
//!
//! When `test_gpio` feature is enabled, this cycles through configured GPIO pins,
//! toggling each in a tight loop for 2 seconds to help identify pin numbers.
//!
//! Configure which pins to test by modifying the `GPIO_PINS_TO_TEST` array below.
//! Pin 12 is excluded as it crashes the device.

use embassy_time::{Duration, Instant};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::Level;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;

/// GPIO pins to test (exclude pin12 as it crashes the device)
/// Modify this array to change which pins are tested
const GPIO_PINS_TO_TEST: &[u8] = &[
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 14, 15, 16, 17, 18, 19, 20, 21,
];

/// Macro to initialize a GPIO pin as output
macro_rules! init_gpio {
    ($num:expr, $periph:expr) => {{
        println!("Initializing GPIO{}...", $num);
        esp_hal::gpio::Output::new($periph, Level::Low, esp_hal::gpio::OutputConfig::default())
    }};
}

/// Macro to test a GPIO pin by toggling it rapidly for 2 seconds
macro_rules! test_gpio {
    ($num:expr, $pin:ident) => {
        println!("Testing GPIO{}...", $num);
        let start_time = Instant::now();
        let mut state = false;
        $pin.set_level(Level::High);
        while start_time.elapsed() < Duration::from_millis(100) {
            state = !state;
            if state {
                $pin.set_level(Level::High);
            } else {
                $pin.set_level(Level::Low);
            }
            // No delay - tight loop for scope visibility
        }
        // Turn off before moving to next pin
        $pin.set_level(Level::Low);
    };
}

/// Run GPIO test mode
///
/// Cycles through configured GPIO pins, toggling each in a tight loop for 2 seconds.
/// Prints which GPIO is currently active.
///
/// To change which pins are tested, modify the `GPIO_PINS_TO_TEST` constant above.
/// Pin 12 is excluded as it crashes the device.
pub async fn run_gpio_test() -> ! {
    println!("GPIO test mode starting...");
    println!("Testing GPIO pins: {:?}", GPIO_PINS_TO_TEST);
    println!("(Pin 12 excluded as it crashes the device)");

    // Initialize hardware
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Allocate heap
    esp_alloc::heap_allocator!(size: 30_000);

    // Start Embassy runtime (needed for embassy_time::Timer)
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    // Initialize all GPIO pins upfront (except GPIO12 which crashes)
    // To change which pins are tested, modify GPIO_PINS_TO_TEST array above
    let mut gpio0 = init_gpio!(0, peripherals.GPIO0);
    let mut gpio1 = init_gpio!(1, peripherals.GPIO1);
    let mut gpio2 = init_gpio!(2, peripherals.GPIO2);
    let mut gpio3 = init_gpio!(3, peripherals.GPIO3);
    let mut gpio4 = init_gpio!(4, peripherals.GPIO4);
    let mut gpio5 = init_gpio!(5, peripherals.GPIO5);
    let mut gpio6 = init_gpio!(6, peripherals.GPIO6);
    let mut gpio7 = init_gpio!(7, peripherals.GPIO7);
    let mut gpio8 = init_gpio!(8, peripherals.GPIO8);
    let mut gpio9 = init_gpio!(9, peripherals.GPIO9);
    let mut gpio10 = init_gpio!(10, peripherals.GPIO10);
    let mut gpio11 = init_gpio!(11, peripherals.GPIO11);
    // GPIO12 excluded - crashes device
    //let mut gpio13 = init_gpio!(13, peripherals.GPIO13);
    let mut gpio14 = init_gpio!(14, peripherals.GPIO14);
    let mut gpio15 = init_gpio!(15, peripherals.GPIO15);
    let mut gpio16 = init_gpio!(16, peripherals.GPIO16);
    let mut gpio17 = init_gpio!(17, peripherals.GPIO17);
    let mut gpio18 = init_gpio!(18, peripherals.GPIO18);
    let mut gpio19 = init_gpio!(19, peripherals.GPIO19);
    let mut gpio20 = init_gpio!(20, peripherals.GPIO20);
    let mut gpio21 = init_gpio!(21, peripherals.GPIO21);

    println!("Initialized {} GPIO pins", GPIO_PINS_TO_TEST.len());

    // Test GPIO pins based on GPIO_PINS_TO_TEST array
    loop {
        for &pin_num in GPIO_PINS_TO_TEST {
            match pin_num {
                0 => {
                    test_gpio!(0, gpio0);
                }
                1 => {
                    test_gpio!(1, gpio1);
                }
                2 => {
                    test_gpio!(2, gpio2);
                }
                3 => {
                    test_gpio!(3, gpio3);
                }
                4 => {
                    test_gpio!(4, gpio4);
                }
                5 => {
                    test_gpio!(5, gpio5);
                }
                6 => {
                    test_gpio!(6, gpio6);
                }
                7 => {
                    test_gpio!(7, gpio7);
                }
                8 => {
                    test_gpio!(8, gpio8);
                }
                9 => {
                    test_gpio!(9, gpio9);
                }
                10 => {
                    test_gpio!(10, gpio10);
                }
                11 => {
                    test_gpio!(11, gpio11);
                }
                12 | 13 => {
                    println!("Skipping GPIO12 (crashes device)");
                }
                14 => {
                    test_gpio!(14, gpio14);
                }
                15 => {
                    test_gpio!(15, gpio15);
                }
                16 => {
                    test_gpio!(16, gpio16);
                }
                17 => {
                    test_gpio!(17, gpio17);
                }
                18 => {
                    test_gpio!(18, gpio18);
                }
                19 => {
                    test_gpio!(19, gpio19);
                }
                20 => {
                    test_gpio!(20, gpio20);
                }
                21 => {
                    test_gpio!(21, gpio21);
                }
                _ => {
                    println!("Warning: GPIO{} not supported, skipping", pin_num);
                }
            }
        }

        println!("Cycle complete, restarting...");
    }
}
