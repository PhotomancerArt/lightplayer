//! The S3's peripheral set — **the blocks before the console** (M6 P04).
//!
//! Every block here is one of three things, and each file says which:
//!
//! - **a view** — a real type with behaviour and events: [`system`] (the
//!   clock gates, the four software interrupts, the hold on core 1),
//!   [`rtc_cntl`] (the reset cause, the stall key, **the RWDT that really
//!   runs**, the super-watchdog), [`timg`] (two counters per group, the
//!   RTC calibration, the MWDT gate), [`systimer`] (the S3's
//!   `Instant::now()`), [`efuse`] (the MAC and the wafer version),
//!   [`i2c_ana_mst`] (the analog master as a `{block, register}` store), and
//!   the two halves of the interrupt matrix ([`crate::intmatrix`]);
//! - **accept-and-remember** — a [`lp_emu_esp_common::RegFile`] seeded from
//!   the PAC's resets, with the bits something spins on pinned to a cited
//!   value ([`accept`]): `SENSITIVE`, `EXTMEM`'s boot registers, `SPI0`,
//!   `SPI1`, `APB_CTRL`, `BB`, `NRX`, `FE`, `FE2`;
//! - **not modelled** — left unmapped on purpose, so a strict run stops on
//!   them: the console (`USB_DEVICE`, P05), the flash cache and SHA (P06),
//!   the pad fabric and RMT (P07).
//!
//! Every constant that is a *guess* is marked `modeled` where it is defined;
//! nothing here is `measured` — no S3 silicon has been read yet.
//!
//! # Which parent each block came from
//!
//! The S3 takes two views from each sibling and the wrong parent is
//! silently wrong (`m6/notes.md` §3): [`rtc_cntl`] and the matrix are the
//! **classic's**, copied with an offset table; [`timg`], [`systimer`] and
//! [`efuse`] are the **C6's**, copied with a counter count, a base and the
//! S3's own wafer-version words. [`system`] is fresh — three chips, three
//! unrelated layouts. The copies live here rather than parameterising the
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

pub mod accept;
pub mod efuse;
pub mod i2c_ana_mst;
pub mod rtc_cntl;
pub mod system;
pub mod systimer;
pub mod timg;
pub mod usb_sj;

use lp_emu_esp_common::StreamId;
use lp_emu_esp_common::periph::BoxedPeripheral;

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

/// The host byte streams a block writes to and reads from. One block has
/// any: the link ([`usb_sj`]).
///
/// `usb_sj` is what a host **received** from the IN endpoint and what it
/// **sent** on the OUT path; `usb_sj_tried` is the observation stream — bytes
/// the guest handed over that no host took. Both are `None` on a machine
/// built with no host side at all, which is what a unit test wants.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostStreams {
    pub usb_sj: Option<StreamId>,
    pub usb_sj_tried: Option<StreamId>,
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
/// and whether a cable is in at power-on.
pub fn boot_set(
    reset_cause: ResetCause,
    identity: EfuseIdentity,
    stall: rtc_cntl::StallKey,
    core1: CoreOneHandle,
    streams: HostStreams,
    usb_host: usb_sj::HostState,
) -> Vec<(u32, u32, BoxedPeripheral)> {
    vec![
        (
            base::SENSITIVE,
            accept::SENSITIVE_LEN,
            Box::new(accept::sensitive()) as BoxedPeripheral,
        ),
        (base::EXTMEM, accept::EXTMEM_LEN, Box::new(accept::extmem())),
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
        (base::SPI0, accept::SPI_LEN, Box::new(accept::spi0())),
        (base::SPI1, accept::SPI_LEN, Box::new(accept::spi1())),
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
    ]
}
