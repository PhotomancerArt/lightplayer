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

/// The DOM-WLE-LAN's Ethernet peripheral set, extracted here because the
/// peripheral singleton is only taken once (see the module doc) and consumed
/// by `super::eth::create_driver` on `net-eth` builds.
///
/// The *pin selection* is this struct's construction site in
/// [`init_board_impl`]; the *drive configuration* (APLL, fixed-link PHY, DMA
/// sizing) is `super::eth`'s. Defined unconditionally — it holds only
/// peripheral singletons, and the non-net `init_board` drops it, which
/// compiles to nothing — so the shared, heavily-commented init body does not
/// have to exist twice under `cfg`.
// `expect`, not `allow`: if the non-net build ever starts reading these
// fields the expectation fails loudly and this attribute gets deleted.
#[cfg_attr(
    not(feature = "net-eth"),
    expect(
        dead_code,
        reason = "constructed and dropped whole by the non-net init_board; the alternative is duplicating the entire documented init body under cfg"
    )
)]
pub struct EthPeripherals {
    /// The EMAC itself.
    pub eth: esp_hal::peripherals::ETH<'static>,
    /// APLL 50 MHz reference clock **output** to the PHY (`EMAC_CLK_OUT_180`).
    ///
    /// GPIO17 is the only working clock topology on this board: no external
    /// oscillator feeds GPIO0, and the GPIO16 output phase fails (bench spike
    /// `spikes/net-bringup-classic`, G1). APLL mode excludes wifi/ESP-NOW —
    /// moot on this image (radio retired), load-bearing for any future one.
    pub clk_out: esp_hal::peripherals::GPIO17<'static>,
    /// RMII RXD0 — fixed by silicon, as are the five below.
    pub rxd0: esp_hal::peripherals::GPIO25<'static>,
    /// RMII RXD1.
    pub rxd1: esp_hal::peripherals::GPIO26<'static>,
    /// RMII CRS_DV.
    pub rx_dv: esp_hal::peripherals::GPIO27<'static>,
    /// RMII TXD0.
    pub txd0: esp_hal::peripherals::GPIO19<'static>,
    /// RMII TXD1.
    pub txd1: esp_hal::peripherals::GPIO22<'static>,
    /// RMII TX_EN.
    pub tx_en: esp_hal::peripherals::GPIO21<'static>,
    /// MDC, parked: no SMI exists on this board (the spike's sweeps proved
    /// GPIO33/GPIO32 inert), but the EMAC driver requires the pins.
    pub mdc: esp_hal::peripherals::GPIO33<'static>,
    /// MDIO, parked — see `mdc`.
    pub mdio: esp_hal::peripherals::GPIO32<'static>,
}

/// Initialize classic-ESP32 hardware (non-net build).
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
#[cfg(not(feature = "net-eth"))]
pub fn init_board() -> (
    SoftwareInterruptControl<'static>,
    TimerGroup<'static, impl TimerGroupInstance>,
    Result<Uart<'static, Blocking>, ConfigError>,
    esp_hal::peripherals::FLASH<'static>,
    esp_hal::peripherals::RMT<'static>,
) {
    // Dropping the Ethernet pins/peripheral is free: singletons carry no
    // state, and nothing configures the pads until `Ethernet::new` runs — so
    // the non-net image's behavior is untouched by their extraction.
    let (sw_int, timg0, uart0, flash, rmt, _eth) = init_board_impl();
    (sw_int, timg0, uart0, flash, rmt)
}

/// Initialize classic-ESP32 hardware (`net-eth` build): everything the
/// non-net variant returns, plus the [`EthPeripherals`] that
/// `super::eth::create_driver` consumes. See the non-net variant's doc for
/// the shared facts.
#[cfg(feature = "net-eth")]
pub fn init_board() -> (
    SoftwareInterruptControl<'static>,
    TimerGroup<'static, impl TimerGroupInstance>,
    Result<Uart<'static, Blocking>, ConfigError>,
    esp_hal::peripherals::FLASH<'static>,
    esp_hal::peripherals::RMT<'static>,
    EthPeripherals,
) {
    init_board_impl()
}

/// The one shared init body behind both `init_board` variants — a tuple
/// cannot carry a `cfg`'d element, and duplicating this function's comments
/// under `cfg` is how they would rot.
fn init_board_impl() -> (
    SoftwareInterruptControl<'static>,
    TimerGroup<'static, esp_hal::peripherals::TIMG0<'static>>,
    Result<Uart<'static, Blocking>, ConfigError>,
    esp_hal::peripherals::FLASH<'static>,
    esp_hal::peripherals::RMT<'static>,
    EthPeripherals,
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

    // Ethernet peripherals, handed out whole; the WHY of each pin is on the
    // struct's field docs. Nothing here touches the pads — that happens (or
    // does not) in `super::eth::create_driver`.
    let eth = EthPeripherals {
        eth: peripherals.ETH,
        clk_out: peripherals.GPIO17,
        rxd0: peripherals.GPIO25,
        rxd1: peripherals.GPIO26,
        rx_dv: peripherals.GPIO27,
        txd0: peripherals.GPIO19,
        txd1: peripherals.GPIO22,
        tx_en: peripherals.GPIO21,
        mdc: peripherals.GPIO33,
        mdio: peripherals.GPIO32,
    };

    (sw_int, timg0, uart0, flash, rmt, eth)
}

/// Start the Embassy runtime with the given timer and software interrupt.
pub fn start_runtime(
    timg0: TimerGroup<'static, impl TimerGroupInstance>,
    sw_int: SoftwareInterruptControl<'static>,
) {
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);
}
