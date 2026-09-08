//! The C6's peripheral set, in three grades.
//!
//! - **modelled** — a real type with behaviour and events: the interrupt
//!   matrix ([`crate::intmatrix`]), [`systimer`], [`timg`] (T0 + the RTC
//!   calibration), [`lp_wdt`] (the RWDT, with a real stage-0 expiry),
//!   [`intpri`] (software interrupts), [`rng`], [`efuse`].
//! - **accept** — a [`lp_emu_esp_common::RegFile`] with a short table of
//!   exceptions ([`accept`]): every block `esp_hal::init` writes and reads
//!   back, with the bits it spins on pinned to the value the discovery cites.
//! - **modelled, P6** — [`uart`] (the FIFOs, the shifter at baud, the
//!   thresholds and receive timeout, a host stream outside), [`usb_sj`] (the
//!   honest USB-Serial-JTAG: host absent, attached-idle or attached-draining,
//!   M6 P2), [`pcr`] (the P5 accept
//!   block, now also feeding the UART clock lines), [`wifi_stub`] (the radio
//!   window as one accept block driven by the spin detector).
//! - **modelled, M4** — [`spi1`] (the legacy flash controller the mask ROM
//!   drives, against a [`crate::flash::FlashImage`]) and [`spi0`] (the cache
//!   MMU's item registers, feeding [`crate::cache::CacheMmu`]).
//! - **modelled, M5 P1** — [`rmt`] (the register file, the 192-word RAM,
//!   two TX engines on the scheduler consuming words at PCR's clock:
//!   position-semantics `tx_lim`, wrap, STOP, `tx_end`/`tx_thr_event`/
//!   `tx_err` on source 49; RX channels accept-and-remember).
//! - **not modelled** — left unmapped on purpose, so a strict run stops on
//!   them: the USB host states M6 has not modelled yet, and where the RMT
//!   waveform goes (M5 P2's signal fabric).
//!
//! Every constant that is a *guess* is marked `modeled` where it is defined;
//! the README's peripheral table repeats the grades.

pub mod accept;
pub mod efuse;
pub mod gpio;
pub mod intpri;
pub mod lp_wdt;
pub mod pcr;
pub mod rmt;
pub mod rng;
pub mod spi0;
pub mod spi1;
pub mod systimer;
pub mod timg;
pub mod uart;
pub mod usb_sj;
pub mod wifi_stub;

use lp_emu_esp_common::StreamId;
use lp_emu_esp_common::periph::BoxedPeripheral;

use crate::cache::CacheHandle;
use crate::flash::FlashHandle;
use crate::intmatrix::{InterruptCore0View, PlicMxView};
use crate::loader::EfuseIdentity;
use crate::memmap::periph as base;

/// The crystal. `PCR.sysclk_conf.clk_xtal_freq` reads 40 and esp-hal derives
/// every derived clock from it (`clocks.rs`, `xtal_clk_frequency`).
pub const XTAL_HZ: u64 = 40_000_000;

/// The RTC slow clock (`RC_SLOW`), **modeled** at 136 kHz — the C6's nominal
/// RC_SLOW frequency, and the number behind the discovery's calibration
/// formula (`40_000_000 * 1024 / 136_000 ≈ 301_176`). Nothing here measured
/// it; it is the value that makes esp-hal's RTC period come out plausible.
pub const RC_SLOW_HZ: u64 = 136_000;

/// The watchdog write-protect key, TIMG and LP_WDT alike (`timg.rs:684`,
/// `rtc_cntl/mod.rs:558`); on the C6 the SWD uses the same key
/// (`rtc_cntl/mod.rs:660`).
pub const WDT_WKEY: u32 = 0x50D8_3AA1;

/// The host streams the console blocks are wired to. The machine registers
/// the streams on the bus first, so the ids exist before the peripherals
/// that hold them do.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostStreams {
    /// UART0's outside: TX bytes go here, RX bytes come from here.
    pub uart0: Option<StreamId>,
    /// UART1's outside. The firmware never opens UART1; a machine that
    /// wants its bytes gives it a stream.
    pub uart1: Option<StreamId>,
    /// USB-Serial-JTAG's outside: what a host **received** from the IN
    /// endpoint (sink) and what it **sent** to the OUT endpoint (source).
    pub usb_sj: Option<StreamId>,
    /// USB-Serial-JTAG's observation stream: bytes the guest handed to the
    /// IN endpoint that no host took (pushed with no host, dropped past a
    /// committed FIFO, dropped by a bus reset).
    pub usb_sj_tried: Option<StreamId>,
}

/// The whole boot set, in [`crate::machine::PERIPHERAL_REGISTRATION_ORDER`].
///
/// `efuse` seeds the EFUSE block; `seed` seeds the RNG; `streams` are the
/// consoles' outsides; `flash` is the chip SPI1 drives and `mmu` the page
/// table SPI0 programs; `usb_host` is the USB host's state at power-on.
/// Everything else is the same on every machine.
pub fn boot_set(
    efuse: EfuseIdentity,
    seed: u64,
    streams: HostStreams,
    flash: FlashHandle,
    mmu: CacheHandle,
    usb_host: usb_sj::HostState,
) -> Vec<(u32, u32, BoxedPeripheral)> {
    let clocks = pcr::UartClockLines::default();
    let rmt_clock = pcr::RmtClockLine::default();
    vec![
        (
            base::LP_APM,
            0x100,
            Box::new(accept::lp_apm()) as BoxedPeripheral,
        ),
        (base::LP_APM0, 0x800, Box::new(accept::lp_apm0())),
        (base::HP_APM, 0x800, Box::new(accept::hp_apm())),
        (base::LP_AON, 0x400, Box::new(accept::lp_aon())),
        (base::PMU, 0x400, Box::new(accept::pmu())),
        (base::LP_CLKRST, 0x400, Box::new(accept::lp_clkrst())),
        (base::LP_WDT, 0x400, Box::new(lp_wdt::LpWdt::new())),
        (base::MODEM_SYSCON, 0x100, Box::new(accept::modem_syscon())),
        (base::MODEM_LPCON, 0x100, Box::new(accept::modem_lpcon())),
        (base::I2C_ANA_MST, 0x100, Box::new(accept::i2c_ana_mst())),
        (
            base::LP_I2C_ANA_MST,
            0x400,
            Box::new(accept::lp_i2c_ana_mst()),
        ),
        (
            base::PCR,
            0x1000,
            Box::new(pcr::Pcr::new(clocks.clone(), rmt_clock.clone())),
        ),
        (base::TIMG0, 0x100, Box::new(timg::Timg::timg0())),
        (base::TIMG1, 0x100, Box::new(timg::Timg::timg1())),
        (base::EFUSE, 0x200, Box::new(efuse::efuse(efuse))),
        (base::LP_TIMER, 0x400, Box::new(accept::lp_timer())),
        (base::APB_SARADC, 0x400, Box::new(accept::apb_saradc())),
        (base::SYSTIMER, 0x100, Box::new(systimer::Systimer::new())),
        (base::ASSIST_DEBUG, 0x400, Box::new(accept::assist_debug())),
        (base::INTERRUPT_CORE0, 0x800, Box::new(InterruptCore0View)),
        (base::PLIC_MX, 0x100, Box::new(PlicMxView)),
        (base::INTPRI, 0x400, Box::new(intpri::Intpri::new())),
        (base::HP_SYS, 0x400, Box::new(accept::hp_sys())),
        (base::TEE, 0x1000, Box::new(accept::tee())),
        (base::LP_TEE, 0x100, Box::new(accept::lp_tee())),
        (base::LP_IO, 0x400, Box::new(accept::lp_io())),
        (base::RNG, 0x400, Box::new(rng::Rng::new(seed))),
        (base::EXTMEM, 0x400, Box::new(accept::extmem())),
        (
            base::UART0,
            0x100,
            Box::new(uart::Uart::uart0(clocks.uart0.clone(), streams.uart0)),
        ),
        (
            base::UART1,
            0x100,
            Box::new(uart::Uart::uart1(clocks.uart1.clone(), streams.uart1)),
        ),
        (
            base::USB_DEVICE,
            0x100,
            Box::new(usb_sj::UsbSerialJtag::new(
                streams.usb_sj,
                streams.usb_sj_tried,
                usb_host,
            )),
        ),
        (base::IO_MUX, 0x100, Box::new(accept::io_mux())),
        (base::GPIO, gpio::LEN, Box::new(gpio::Gpio::new())),
        (base::SPI0, 0x400, Box::new(spi0::Spi0::new(mmu))),
        (base::SPI1, 0x400, Box::new(spi1::Spi1::new(flash))),
        // Registers, the gap, and the RAM at `+0x400..+0x700` — P5's accept
        // block was `0x400` long and left the RAM unmapped (M5 discovery C4;
        // M4's upload walk hit the other end of it, as a strict stop at
        // `0x6000_6400` from `Ws281xDriver::open`).
        (base::RMT, rmt::LEN, Box::new(rmt::Rmt::new(rmt_clock))),
        // The radio window, after RMT: `Rmt::new` runs before esp-radio's
        // init in `main`, so this is the order the boot meets them.
        (
            base::MODEM_WINDOW,
            wifi_stub::WINDOW_LEN,
            Box::new(wifi_stub::WifiStub::new()),
        ),
        (
            base::WIFI_PWR,
            base::WIFI_PWR_LEN,
            Box::new(wifi_stub::WifiStub::pwr()),
        ),
        // The PHY's I2C burst command memory, after the radio window: the
        // first block `phy_i2c_master_cmd_mem_init` reaches past it.
        (
            base::I2C_MST_MEM,
            base::I2C_MST_MEM_LEN,
            Box::new(wifi_stub::WifiStub::i2c_mst_mem()),
        ),
    ]
}
