//! The S3's peripheral set — **the blocks before the console** (M6 P04),
//! **the console** (M6 P05), and **the flash, the cache and the mask ROM's
//! console** (M6 P06).
//!
//! Every block here is one of three things, and each file says which:
//!
//! - **a view** — a real type with behaviour and events: [`system`] (the
//!   clock gates, the four software interrupts, the hold on core 1),
//!   [`rtc_cntl`] (the reset cause, the stall key, **the RWDT that really
//!   runs**, the super-watchdog), [`timg`] (two counters per group, the
//!   RTC calibration, the MWDT gate), [`systimer`] (the S3's
//!   `Instant::now()`), [`efuse`] (the MAC and the wafer version),
//!   [`i2c_ana_mst`] (the analog master as a `{block, register}` store),
//!   [`usb_sj`] (**the link, and the console on it**), the two halves of
//!   the interrupt matrix ([`crate::intmatrix`]), and since P06 [`spi1`]
//!   (the flash controller on `engine::spi_flash`), [`spi0`] (the cache's
//!   port, which refuses), [`extmem`] (the cache controller, whose enable
//!   bits reach [`crate::cache`]), [`flash_mmu`] (the page table at
//!   `0x600C_5000`), [`sha`] (the C6's IP on `engine::sha`) and [`uart`]
//!   (the mask ROM's console on `engine::uart`);
//! - **accept-and-remember** — a [`lp_emu_esp_common::RegFile`] seeded from
//!   the PAC's resets, with the bits something spins on pinned to a cited
//!   value ([`accept`]): `SENSITIVE`, `APB_CTRL`, `BB`, `NRX`, `FE`, `FE2`,
//!   and — P07's blocks, here because the ROM-up boot meets them — `GPIO`
//!   (with the strapping word) and `IO_MUX`;
//! - **not modelled** — left unmapped on purpose, so a strict run stops on
//!   them: the pad fabric and RMT (P07).
//!
//! ⚠️ [`usb_sj`] is the one block here that is **not** the S3's own file: it
//! is the C6's view, moved to [`lp_emu_esp_common::ip::usb_sj`] and
//! parameterised (ruling D1 (b) / DD64), because the two chips' PACs agree on
//! that layout offset-for-offset. The S3's file is the chip's parameters and
//! nothing else — and the two registers this part does not have sit behind a
//! capability it withholds.
//!
//! Every constant that is a *guess* is marked `modeled` where it is defined;
//! nothing here is `measured` — no S3 silicon has been read yet.
//!
//! # Which parent each block came from
//!
//! The S3 takes two views from each sibling and the wrong parent is
//! silently wrong (`m6/notes.md` §3): [`rtc_cntl`] and the matrix are the
//! **classic's**, copied with an offset table; [`timg`], [`systimer`],
//! [`efuse`], [`spi1`] and [`sha`] are the **C6's**, copied with a counter
//! count, a base, the S3's own wafer-version words, and — for SHA — the
//! wider `h_mem`. [`system`] and [`uart`] are fresh — three chips, three
//! unrelated layouts. [`spi0`] and [`extmem`] are the *classic's* shape
//! (a refusing cache port; a split-cache block with a table beside it) at
//! the S3's numbers. The copies live here rather than parameterising the
//! siblings' files because the plan's invariant is that the C6 and the
//! classic do not move by a byte; the extraction into `lp-emu-esp-common`
//! is M8's, and each file names what it would extract.
//!
//! # Registration order is a contract
//!
//! [`crate::machine::PERIPHERAL_REGISTRATION_ORDER`] is the order the blocks
//! are added to the bus, and [`boot_set`] produces exactly it. The bus packs
//! a peripheral's index into every scheduler event id, so re-ordering the
//! list re-points already-scheduled events; a block a later phase adds goes
//! **in its place in the list** — where the boot meets it — never appended
//! for convenience.
//!
//! ⚠️ **`USB_DEVICE` is the exception, and it is appended.** The boot meets
//! the console a few hundred cycles *before* it meets `SYSTIMER`, which P04
//! registered last on the expectation that `Instant::now()` would be the last
//! thing reached. Putting the console in the boot's order would move
//! `SYSTIMER`'s index — which is the thing this rule exists to prevent — so
//! the index contract wins and the list's comment carries the narrative.
//! P06's six new blocks follow it for the same reason: every one of them is
//! met by the **ROM-up** boot before `SENSITIVE`, and re-sorting the list to
//! that boot's order would move every index P04 and P05 pinned.

pub mod accept;
pub mod efuse;
pub mod extmem;
pub mod flash_mmu;
pub mod gpio;
pub mod i2c_ana_mst;
pub mod io_mux;
pub mod rmt;
pub mod rng;
pub mod rtc_cntl;
pub mod sha;
pub mod spi0;
pub mod spi1;
pub mod system;
pub mod systimer;
pub mod timg;
pub mod uart;
pub mod usb_sj;

use lp_emu_esp_common::StreamId;
use lp_emu_esp_common::periph::BoxedPeripheral;

use crate::cache::CacheHandle;
use crate::flash::FlashHandle;
use crate::intmatrix::InterruptCoreView;
use crate::loader::{EfuseIdentity, ResetCause};
use crate::machine::CoreOneHandle;
use crate::memmap::periph as base;

/// The crystal: **40 MHz**. esp-hal's S3 clock tree forces it —
/// `ClockConfig::configure` sets `XtalClkConfig::_40` with a `TODO: support
/// multiple crystal frequencies` beside it (`soc/esp32s3/clocks.rs:119-123`)
/// — so on this chip the crystal is not measured by a calibration the way
/// the classic's is; it is what esp-hal assumes.
pub const XTAL_HZ: u64 = 40_000_000;

/// APB, **80 MHz** — `apb_clk` in the generated S3 clock tree
/// (`esp-metadata-generated-0.4.0/src/_generated_esp32s3.rs`), and the
/// TIMG counters' source clock when a group's `use_xtal` bit is clear.
pub const APB_HZ: u64 = 80_000_000;

/// RC_SLOW, **136 kHz** — *modeled*. esp-hal's own S3 clock tree calls it
/// "136k RC_SLOW" and says in the same breath that it is not calibrated
/// there and can only be estimated (`soc/esp32s3/clocks.rs:8-10`). It is
/// the number behind the TIMG calibration's answer and therefore behind the
/// `store1` period the RWDT's timeout is derived from; nothing here measured
/// it.
pub const RC_SLOW_HZ: u64 = 136_000;

/// The watchdog write-protect key, `0x50D8_3AA1` — the same number on the
/// MWDTs and the RWDT (`rtc_cntl/mod.rs:558-563`, `timer/timg.rs`), and the
/// PAC's own reset value for both `TIMG0.wdtwprotect` and
/// `RTC_CNTL.wdtwprotect`, so both come out of reset **unlocked**.
pub const WDT_WKEY: u32 = 0x50D8_3AA1;

/// The super-watchdog's key, **`0x8F1D_312A`** — a different number from the
/// RWDT's on this chip (`rtc_cntl/mod.rs:656-660`: `#[cfg(not(any(esp32c6,
/// esp32h2)))] 0x8F1D_312A`; the C6 reuses the RWDT's), and again the PAC's
/// reset for `swd_wprotect` (`esp32s3-0.35.2/src/rtc_cntl/swd_wprotect.rs:44`),
/// so the SWD too is unlocked at power-on.
pub const SWD_WKEY: u32 = 0x8F1D_312A;

/// The host byte streams a block writes to and reads from.
///
/// `usb_sj` is what a host **received** from the IN endpoint and what it
/// **sent** on the OUT path; `usb_sj_tried` is the observation stream — bytes
/// the guest handed over that no host took. `uart0` is the mask ROM's
/// console (P06): what left UART0's shifter, and what a host on that wire
/// sent. All are `None` on a machine built with no host side at all, which
/// is what a unit test wants.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostStreams {
    pub usb_sj: Option<StreamId>,
    pub usb_sj_tried: Option<StreamId>,
    pub uart0: Option<StreamId>,
}

/// The shared state the flash-era blocks hold between them (P06): the
/// chip, the cache model, and the strapping word.
pub struct FlashSet {
    /// The flash chip — SPI1 executes commands against it and the cache
    /// fill reads through it.
    pub flash: FlashHandle,
    /// The cache-enable bits, the MMU table and D4's watch slot, shared
    /// between `EXTMEM`, `FLASH_MMU` and the machine.
    pub cache: CacheHandle,
    /// What `GPIO.strap` reads — the pads as latched at reset (`--strap`).
    pub strap: u32,
    /// The machine's `--seed`, for the RNG the bootloader reads.
    pub seed: u64,
}

/// The whole boot set, in [`crate::machine::PERIPHERAL_REGISTRATION_ORDER`].
///
/// `reset_cause` is what a direct load asserts into `RTC_CNTL.reset_state`
/// (loader item 6); `identity` is the part this run claims to be (loader
/// item 7); `stall` is the handle RTC_CNTL publishes its half of the CPU
/// stall key through and the machine reads; `core1` is
/// `SYSTEM.core_1_control_0`, the register the machine's hold on slot 1
/// already reads, now shared with the view that lets a guest write it;
/// `streams` and `usb_host` are the link's (M6 P05) — where its bytes go,
/// and whether a cable is in at power-on; `flash` is P06's shared state.
pub fn boot_set(
    reset_cause: ResetCause,
    identity: EfuseIdentity,
    stall: rtc_cntl::StallKey,
    core1: CoreOneHandle,
    streams: HostStreams,
    usb_host: usb_sj::HostState,
    flash: &FlashSet,
) -> Vec<(u32, u32, BoxedPeripheral)> {
    vec![
        (
            base::SENSITIVE,
            accept::SENSITIVE_LEN,
            Box::new(accept::sensitive()) as BoxedPeripheral,
        ),
        (
            base::EXTMEM,
            accept::EXTMEM_LEN,
            Box::new(extmem::Extmem::new(flash.cache.clone())),
        ),
        (
            base::INTERRUPT_CORE1,
            crate::intmatrix::VIEW_LEN,
            Box::new(InterruptCoreView::core1()),
        ),
        (
            base::INTERRUPT_CORE0,
            crate::intmatrix::VIEW_LEN,
            Box::new(InterruptCoreView::core0()),
        ),
        (
            base::RTC_CNTL,
            rtc_cntl::RTC_CNTL_LEN,
            Box::new(rtc_cntl::RtcCntl::new(reset_cause, stall)),
        ),
        (
            base::SYSTEM,
            system::SYSTEM_LEN,
            Box::new(system::SystemView::new(core1)),
        ),
        (
            base::EFUSE,
            efuse::EFUSE_LEN,
            Box::new(efuse::efuse(identity)),
        ),
        (
            base::I2C_ANA_MST,
            i2c_ana_mst::I2C_ANA_MST_LEN,
            Box::new(i2c_ana_mst::I2cAnaMst::new()),
        ),
        (base::TIMG0, timg::TIMG_LEN, Box::new(timg::Timg::timg0())),
        (
            base::APB_CTRL,
            accept::APB_CTRL_LEN,
            Box::new(accept::apb_ctrl()),
        ),
        (base::SPI0, accept::SPI_LEN, Box::new(spi0::Spi0::new())),
        (
            base::SPI1,
            accept::SPI_LEN,
            Box::new(spi1::Spi1::new(flash.flash.clone())),
        ),
        (base::BB, accept::RF_LEN, Box::new(accept::bb())),
        (base::NRX, accept::RF_LEN, Box::new(accept::nrx())),
        (base::FE, accept::RF_LEN, Box::new(accept::fe())),
        (base::FE2, accept::RF_LEN, Box::new(accept::fe2())),
        (base::TIMG1, timg::TIMG_LEN, Box::new(timg::Timg::timg1())),
        (
            base::SYSTIMER,
            systimer::SYSTIMER_LEN,
            Box::new(systimer::Systimer::new()),
        ),
        (
            base::USB_DEVICE,
            usb_sj::USB_DEVICE_LEN,
            Box::new(usb_sj::new(streams.usb_sj, streams.usb_sj_tried, usb_host)),
        ),
        // ---- P06: the ROM-up boot's blocks, appended (module docs) ----
        (
            base::FLASH_MMU,
            flash_mmu::LEN,
            Box::new(flash_mmu::FlashMmuView::new(flash.cache.clone())),
        ),
        (base::SHA, sha::LEN, Box::new(sha::Sha::new())),
        (
            base::UART0,
            uart::UART_LEN,
            Box::new(uart::Uart::uart0(streams.uart0)),
        ),
        (
            base::UART1,
            uart::UART_LEN,
            Box::new(uart::Uart::uart1(None)),
        ),
        // ---- P07: the pads and the RMT, where the accept blocks were ----
        (base::GPIO, gpio::LEN, Box::new(gpio::Gpio::new(flash.strap))),
        (base::IO_MUX, io_mux::LEN, Box::new(io_mux::IoMux::new())),
        // Registers, the gap, and the RAM at `+0x800..+0xe00` — P06's accept
        // block was `0x100` long and left the RAM unmapped on purpose.
        (base::RMT, rmt::LEN, Box::new(rmt::new())),
        (
            base::ASSIST_DEBUG,
            accept::ASSIST_DEBUG_LEN,
            Box::new(accept::assist_debug()),
        ),
        (
            base::APB_SARADC,
            accept::APB_SARADC_LEN,
            Box::new(accept::apb_saradc()),
        ),
        (base::SENS, accept::SENS_LEN, Box::new(accept::sens())),
        (base::RNG, rng::LEN, Box::new(rng::Rng::new(flash.seed))),
    ]
}
