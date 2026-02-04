//! RMT driver test mode
//!
//! When `test_rmt` feature is enabled, this runs simple LED patterns
//! to verify the RMT driver works correctly.

use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::rmt::Rmt;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;

use crate::output::{LedChannel, LedTransaction, rmt_ws2811_wait_complete, rmt_ws2811_write_bytes};

/// Run RMT test mode
///
/// Displays simple patterns on LEDs to verify RMT driver works.
pub async fn run_rmt_test() -> ! {
    println!("RMT test mode starting...");

    // Initialize hardware (similar to init_board but we need peripherals)
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Allocate heap
    esp_alloc::heap_allocator!(size: 300_000);

    // Start Embassy runtime (needed for embassy_time::Timer)
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    // Configure RMT
    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80)).expect("Failed to initialize RMT");

    // Use GPIO18 (pin 10 on board) for LED output (hardcoded for testing)
    // GPIO pins implement PeripheralOutput trait, so we can pass directly
    let pin = peripherals.GPIO18;

    // Initialize RMT driver for 8 LEDs
    const NUM_LEDS: usize = 256;
    let channel = LedChannel::new(rmt, pin, NUM_LEDS).expect("Failed to initialize LED channel");

    println!("RMT driver initialized (LedChannel created), starting chase pattern...");
    // Note: LedChannel is created to exercise Phase 2/3, but using old API for now
    // TODO: In Phase 4, we'll use channel.start_transmission() and tx.wait_complete()

    loop {
        // Chase pattern - white dot moving down the strip
        println!("Chase pattern");
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
            rmt_ws2811_write_bytes(&data);
            rmt_ws2811_wait_complete();
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
