use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::timer::timg::{TimerGroup, TimerGroupInstance};

/// Initialize ESP32-C6 hardware
///
/// Sets up CPU clock, timers, and other board-specific hardware.
/// Returns runtime components needed for Embassy and hardware peripherals.
/// FLASH peripheral is included for persistent storage (default; disabled with memory_fs feature).
pub fn init_board() -> (
    SoftwareInterruptControl<'static>,
    TimerGroup<'static, impl TimerGroupInstance>,
    esp_hal::peripherals::RMT<'static>,
    esp_hal::peripherals::USB_DEVICE<'static>,
    esp_hal::peripherals::GPIO18<'static>,
    esp_hal::peripherals::FLASH<'static>,
    esp_hal::peripherals::GPIO4<'static>,
) {
    // Configure CPU clock to maximum speed (160MHz for ESP32-C6)
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Allocate heap. Reserve headroom for main task stack (Cranelift JIT lowering is stack-heavy).
    // It's a balance between stack and heap, and this was chosen based on empirical testing.
    esp_alloc::heap_allocator!(size: 320_000);

    // Extract peripherals we need before moving others
    let rmt = peripherals.RMT;
    let usb_device = peripherals.USB_DEVICE;
    let gpio18 = peripherals.GPIO18;
    let flash = peripherals.FLASH;
    let gpio4 = peripherals.GPIO4;

    // Set up software interrupt and timer for Embassy runtime
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);

    (sw_int, timg0, rmt, usb_device, gpio18, flash, gpio4)
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
