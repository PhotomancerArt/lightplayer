//! ESP32-C6 specific board initialization
//!
//! This module contains board-specific code for ESP32-C6.
//! To add support for another board (e.g., ESP32-C3), create a similar file
//! and add feature gates in board/mod.rs.

use esp_hal::clock::CpuClock;
use esp_hal::{
    interrupt::software::SoftwareInterruptControl,
    timer::timg::{TimerGroup, TimerGroupInstance},
};

/// Initialize ESP32-C6 hardware
///
/// Sets up CPU clock, timers, and other board-specific hardware.
/// Returns runtime components needed for Embassy and hardware peripherals.
pub fn init_board() -> (
    SoftwareInterruptControl<'static>,
    TimerGroup<'static, impl TimerGroupInstance>,
    esp_hal::peripherals::RMT<'static>,
    esp_hal::peripherals::USB_DEVICE<'static>,
    esp_hal::peripherals::GPIO18<'static>,
) {
    // Configure CPU clock to maximum speed (160MHz for ESP32-C6)
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Allocate heap
    esp_alloc::heap_allocator!(size: 340_000);

    // Extract peripherals we need before moving others
    let rmt = peripherals.RMT;
    let usb_device = peripherals.USB_DEVICE;
    let gpio18 = peripherals.GPIO18;

    // Set up software interrupt and timer for Embassy runtime
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);

    (sw_int, timg0, rmt, usb_device, gpio18)
}

/// Start Embassy runtime
///
/// Starts the Embassy async runtime with the given timer and software interrupt.
pub fn start_runtime(
    timg0: TimerGroup<'static, impl TimerGroupInstance>,
    sw_int: SoftwareInterruptControl<'static>,
) {
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);
}
