//! Classic ESP32 (LX6) board initialization.
//!
//! Ported from `fw-esp32s3/src/board/esp32s3/init.rs`. The shape is the S3's;
//! the peripheral list is not:
//!
//! - **UART0 instead of `USB_DEVICE`.** This chip has no USB-Serial-JTAG
//!   peripheral; the host link is UART0 through the board's CH340K bridge.
//! - **No `Rwdt`.** fw-esp32s3 hands the RTC watchdog to its recovery
//!   subsystem; this crate has no `lp-recovery` backend yet (see `Cargo.toml`),
//!   and arming a watchdog with nothing feeding it would reset the board on a
//!   timer. M7 adds both together, the way the S3 has them.
//! - **`RMT` is handed straight back.** Unlike the S3's `init_board`, this one
//!   does not construct the `Rmt` driver: the clock rate it wants belongs to
//!   `output::rmt::shared_driver::RMT_CLOCK`, and `main.rs` is where a failure
//!   to init it turns into "no LED output this boot" rather than "no boot".
//!   Same shape as fw-esp32s3, which also returns the raw peripheral.
//!
//! ⚠️ `init_board` takes the `esp_hal` peripheral singleton, and taking it
//! twice panics. It is the app path's **only** call to `esp_hal::init`.

use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::timer::timg::{TimerGroup, TimerGroupInstance};
use esp_hal::uart::{Config as UartConfig, Uart};
use esp_hal::{Blocking, uart::ConfigError};

/// Initialize classic-ESP32 hardware.
///
/// Sets up the CPU clock and returns the runtime components the app layer
/// needs: the software-interrupt control and timer group for the executor,
/// UART0 for `serial::io_task`, the FLASH peripheral for `flash_storage`, and
/// the RMT peripheral for `output::rmt`.
///
/// The UART is returned in [`Blocking`] mode; `io_task` converts it with
/// `into_async()`. Constructing it here rather than there is deliberate — see
/// the baud-divisor note below, which has to happen before the first
/// `esp_println!`.
///
/// Unlike the C6, the heap is **not** allocated here — `main.rs` owns it.
pub fn init_board() -> (
    SoftwareInterruptControl<'static>,
    TimerGroup<'static, impl TimerGroupInstance>,
    Result<Uart<'static, Blocking>, ConfigError>,
    esp_hal::peripherals::FLASH<'static>,
    esp_hal::peripherals::RMT<'static>,
    esp_hal::peripherals::CPU_CTRL<'static>,
) {
    // `esp_hal::init` disables the RTC watchdog (RWDT) and both TIMG
    // watchdogs unconditionally (esp-hal 1.1.1 `lib.rs::init`). `CpuClock::max()`
    // is 240 MHz on this chip, matching the S3's `init_board`; printed and
    // measured timings assume the fast clock.
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let flash = peripherals.FLASH;
    // Handed on untouched: `main.rs` builds the `Rmt` driver at the WS281x
    // clock rate and registers the driver, so an RMT failure costs the board
    // its LED output rather than its boot.
    let rmt = peripherals.RMT;
    // Handed on untouched, like RMT: `main.rs` starts the APP core with the
    // RMT refill ISR on it (`output::rmt::shared_driver::start_app_core_isr`),
    // and a failure there costs the board its dual-core deployment — it falls
    // back to the single-core semantics — rather than its boot.
    let cpu_ctrl = peripherals.CPU_CTRL;

    // ⚠️ Load-bearing twice over.
    //
    // 1. It is the server transport. UART0 through the CH340K bridge is this
    //    chip's only host link — there is no USB-Serial-JTAG. `serial::io_task`
    //    takes this binding, `into_async()`s it and splits RX/TX.
    //
    // 2. It programs the baud divisor that `esp-println` depends on but never
    //    sets. `esp-println`'s `uart` feature writes UART0's TX FIFO directly;
    //    the ROM leaves a divisor computed for its own pre-reclock clock tree,
    //    and `esp_hal::init` above has just moved the CPU to 240 MHz, so until
    //    something reprograms the divisor every `esp_println!` prints garbage
    //    at any standard host baud. Constructing this `Uart` is what
    //    reprograms it (experiment repo FINDINGS.md "C1", diagnosed on this
    //    exact chip). Which is why this happens HERE and not in `io_task`:
    //    the `[INIT]` lines `main.rs` prints on the way to spawning that task
    //    would otherwise be unreadable.
    //
    // 921600 8N1 — `lpc_model::DEFAULT_SERIAL_BAUD_RATE`, the baud every
    // lp-cli/Studio serial connect opens with. On the C6/S3 that constant is
    // a fiction (native USB has no baud); this chip is the first where it
    // hits a real wire, and the mismatch was found the hard way: the host
    // opened 921600 against a 115200 firmware and heard NOTHING (CH340 drops
    // framing-error bytes), which read as `NoSerialOutput`. The CH340K is
    // comfortable at 921600, and the 8x line rate is the difference between
    // ~1.4 s and ~175 ms for a 16 KiB ProjectRead. Console consequence:
    // `espflash monitor` needs `--baud 921600` (the flash recipes in the
    // README carry it); the ROM's own boot banner still goes out at 115200
    // and will look garbled in such a monitor — that is cosmetic.
    // TX=GPIO1 / RX=GPIO3 are the classic devkit's UART0 bridge pins, and
    // are the two GPIOs the DOM-Z-102 board manifest reserves for exactly
    // this reason.
    //
    // The error is returned rather than `expect`ed: a UART config failure
    // costs the board its host link, not its boot, and a board that boots far
    // enough to blink is more diagnosable than one that resets in a loop.
    let uart0 = Uart::new(
        peripherals.UART0,
        UartConfig::default().with_baudrate(921_600),
    )
    .map(|uart| uart.with_tx(peripherals.GPIO1).with_rx(peripherals.GPIO3));

    // Set up software interrupt and timer for the Embassy runtime.
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);

    (sw_int, timg0, uart0, flash, rmt, cpu_ctrl)
}

/// Start the Embassy runtime with the given timer and software interrupt.
pub fn start_runtime(
    timg0: TimerGroup<'static, impl TimerGroupInstance>,
    sw_int: SoftwareInterruptControl<'static>,
) {
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);
}
