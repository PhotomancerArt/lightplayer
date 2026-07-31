//! ESP32-S3 board initialization.
//!
//! Ported from `fw-esp32c6/src/board/esp32c6/init.rs`. The shape is the C6's;
//! the peripheral list is not, because this board brings up no button GPIO and
//! no radio. Those return values are added when the drivers that consume them
//! are.
//!
//! ⚠️ `init_board` takes the `esp_hal` peripheral singleton, and taking it twice
//! panics. It is the app path's **only** call to `esp_hal::init`: `main.rs`'s
//! `boot_firmware` calls this and nothing else, and the harness entrypoint that
//! does call `esp_hal::init` directly is `cfg`-exclusive with it. Do not add a
//! second call on either path.

use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::rtc_cntl::{Rtc, Rwdt};
use esp_hal::timer::timg::{TimerGroup, TimerGroupInstance};

/// Initialize ESP32-S3 hardware.
///
/// Sets up the CPU clock and returns the runtime components the app layer
/// needs: the software-interrupt control and timer group for the executor, the
/// USB-Serial-JTAG peripheral for `serial::io_task`, the FLASH peripheral for
/// `flash_storage`, the RTC watchdog, and the RMT peripheral for the WS281x
/// output driver.
///
/// The RTC watchdog is returned unarmed; the recovery subsystem (P3) arms it.
/// The RMT peripheral is returned raw — `Rmt::new` needs the clock rate, which
/// is the output driver's fact, not the board's.
///
/// Unlike the C6, the heap is **not** allocated here — `main.rs` owns it. See
/// the module doc.
pub fn init_board() -> (
    SoftwareInterruptControl<'static>,
    TimerGroup<'static, impl TimerGroupInstance>,
    esp_hal::peripherals::USB_DEVICE<'static>,
    esp_hal::peripherals::FLASH<'static>,
    Rwdt,
    esp_hal::peripherals::RMT<'static>,
) {
    // Configure CPU clock to maximum speed (240 MHz for ESP32-S3).
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Extract peripherals we need before moving others.
    let usb_device = peripherals.USB_DEVICE;
    let flash = peripherals.FLASH;
    let rmt = peripherals.RMT;

    // Set up software interrupt and timer for the Embassy runtime.
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);

    // RTC watchdog for the crash-recovery backstop (armed later by recovery).
    let rtc = Rtc::new(peripherals.LPWR);
    let rwdt = rtc.rwdt;

    (sw_int, timg0, usb_device, flash, rwdt, rmt)
}

/// Start the Embassy runtime with the given timer and software interrupt.
pub fn start_runtime(
    timg0: TimerGroup<'static, impl TimerGroupInstance>,
    sw_int: SoftwareInterruptControl<'static>,
) {
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);
}
