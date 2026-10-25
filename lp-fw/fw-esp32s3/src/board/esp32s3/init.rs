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
/// `flash_storage`, the RTC watchdog, and the WS281x output driver's
/// peripherals (`crate::output::OutputPeripherals`: the RMT, or LCD_CAM and a
/// GDMA channel under `lcd-ws281x`).
///
/// The RTC watchdog is returned unarmed; the recovery subsystem (P3) arms it.
/// The output peripherals are returned raw — their clocks are the output
/// driver's fact, not the board's.
///
/// Unlike the C6, the heap is **not** allocated here — `main.rs` owns it. See
/// the module doc.
pub fn init_board() -> (
    SoftwareInterruptControl<'static>,
    TimerGroup<'static, impl TimerGroupInstance>,
    esp_hal::peripherals::USB_DEVICE<'static>,
    esp_hal::peripherals::FLASH<'static>,
    Rwdt,
    crate::output::OutputPeripherals,
) {
    // Arm coprocessor 0 before anything can JIT a Float-mode shader (M7 D6).
    // Silicon has been measured arriving armed under this boot chain, but the
    // provenance of that write is unpinned, so this does not depend on it —
    // see `super::fpu`. Read-modify-write, so a board that booted with every
    // coprocessor enabled keeps them.
    //
    // Embassy tasks share this context, so one call covers the shader task;
    // a second core or a context with its own `CPENABLE` would need its own.
    let cpenable = super::fpu::arm();
    log::debug!("FPU armed: CPENABLE = {cpenable:#010x}");

    // Configure CPU clock to maximum speed (240 MHz for ESP32-S3).
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Extract peripherals we need before moving others.
    let usb_device = peripherals.USB_DEVICE;
    let flash = peripherals.FLASH;
    let outputs = crate::output::OutputPeripherals {
        rmt: peripherals.RMT,
        #[cfg(feature = "lcd-ws281x")]
        lcd_cam: peripherals.LCD_CAM,
        #[cfg(feature = "lcd-ws281x")]
        dma: peripherals.DMA_CH0,
    };
    // The radio probe's two peripherals, kept until the scheduler runs.
    #[cfg(all(feature = "radio_boot_probe", not(fw_harness)))]
    crate::radio_boot_probe::stash(peripherals.WIFI, peripherals.BT);
    // The radio's peripherals, kept until `core_boot` brings the radio up
    // after the filesystem mounts (`crate::radio_peripherals`).
    #[cfg(all(s3_radio, not(fw_harness)))]
    crate::radio_peripherals::stash(peripherals.WIFI, peripherals.BT);
    // Core 1's control, kept until `core_boot` starts the render helper.
    #[cfg(s3_two_core)]
    crate::core1::stash(peripherals.CPU_CTRL);
    // Core 1's control, kept until `core_boot` starts the PC sampler on it.
    #[cfg(all(feature = "pc-sampler", not(fw_harness)))]
    crate::pc_sampler::stash(peripherals.CPU_CTRL);
    // The PSRAM's peripheral, kept until `core_boot` brings it up.
    #[cfg(all(feature = "psram", not(fw_harness)))]
    crate::psram::stash(peripherals.PSRAM);

    // Set up software interrupt and timer for the Embassy runtime.
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);

    // RTC watchdog for the crash-recovery backstop (armed later by recovery).
    let rtc = Rtc::new(peripherals.LPWR);
    let rwdt = rtc.rwdt;

    (sw_int, timg0, usb_device, flash, rwdt, outputs)
}

/// Start the Embassy runtime with the given timer and software interrupt.
pub fn start_runtime(
    timg0: TimerGroup<'static, impl TimerGroupInstance>,
    sw_int: SoftwareInterruptControl<'static>,
) {
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);
}
