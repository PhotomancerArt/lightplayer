//! The classic's peripheral set — in P3, **accept-and-remember blocks only**.
//!
//! Every block here is a [`lp_emu_esp_common::RegFile`] seeded from the
//! generated `regs/` reset values ([`accept`]). None has behaviour. That is
//! the technique of the strict bring-up pass, not a shortcut: an accept block
//! is a **probe** — it lets the boot get past a block so the *next* strict
//! stop is visible, and the order the blocks were needed in is the order
//! P4–P8 model them. `docs/reports/2026-09-10-esp32v3-strict-boot-inventory.md`
//! is that ledger.
//!
//! The grades, one line each, as the C6's `periph/mod.rs` carries them:
//!
//! - **accept** — [`accept`]: every block the strict run reached, with the
//!   bits it spins on pinned to a cited value or reported as an E-premise
//!   stop. All of them; see the module for which phase owns which.
//! - **not modelled** — left unmapped on purpose, so a strict run stops on
//!   them: everything the boot has not reached yet.
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
pub mod dport;
pub mod efuse;
pub mod flash_mmu;
pub mod i2c_ana_mst;
pub mod rtc_cntl;
pub mod timg;

use lp_emu_esp_common::periph::BoxedPeripheral;

use crate::cache::CacheHandle;
use crate::loader::{EfuseIdentity, ResetCause};
use crate::memmap::periph as base;
use crate::memmap::{self};

/// The desk board's crystal: **40 MHz** (`../bench.md`, "40 MHz crystal").
///
/// The classic ships with either 26 or 40 MHz and the firmware *measures*
/// which through the TIMG calibration ([`timg`]), so this is the number the
/// model is calibrating against rather than one it asserts to the guest.
pub const XTAL_HZ: u64 = 40_000_000;

/// APB, **80 MHz** — `apb_clk_80m_frequency` in the generated clock tree
/// (`esp-metadata-generated-0.4.0/src/_generated_esp32.rs`), and what
/// `esp_hal::init` leaves the tree at on this image (P3's ledger §3.1: the
/// CPU ends up at 240 MHz on the PLL and APB at 80). Every TIMG counter's
/// tick comes from here through its own prescaler.
pub const APB_HZ: u64 = 80_000_000;

/// RC_FAST, **8 MHz** — `rc_fast_clk_frequency` in the same generated file.
pub const RC_FAST_HZ: u64 = 8_000_000;

/// RC_FAST divided by 256 — `rc_fast_div_clk_frequency` in the same file,
/// and the calibration clock `detect_xtal_freq` picks (`RcFastDivClk`).
pub const RC_FAST_DIV_HZ: u64 = RC_FAST_HZ / 256;

/// RC_SLOW, **150 kHz** — `rc_slow_clk_frequency` in the same file, and the
/// calibration clock `calibrate_rtc_slow_clock` picks (`RcSlowClk`).
pub const RC_SLOW_HZ: u64 = 150_000;

/// The 32.768 kHz crystal input — `xtal32k_clk_frequency` in the same file.
/// The desk board has no 32k crystal; the rate is here because
/// `rtc_cali_clk_sel = 2` selects it and a model that answered the wrong
/// clock silently would be worse than one that answers this.
pub const XTAL32K_HZ: u64 = 32_768;

/// The watchdog write-protect key, `0x50D8_3AA1` — the same number on the
/// MWDTs and the RWDT, and the PAC's own reset value for both
/// `TIMG0.wdtwprotect` and `RTC_CNTL.wdtwprotect`, so both come out of reset
/// **unlocked**.
pub const WDT_WKEY: u32 = 0x50D8_3AA1;

/// The whole boot set, in [`crate::machine::PERIPHERAL_REGISTRATION_ORDER`].
///
/// `reset_cause` is what a direct load asserts (loader item 7); `identity`
/// is the part this run claims to be (MAC and chip revision), which reaches
/// two blocks — the eFuse view and, for the revision's top bit, `APB_CTRL`;
/// `stall` is the handle RTC_CNTL publishes its half of the CPU stall key
/// through, which the machine reads and to which P4's DPORT view adds its
/// third input; `cache` and `appcpu` are the two states the machine shares
/// with DPORT's view and with the flash MMU tables (P4).
pub fn boot_set(
    reset_cause: ResetCause,
    identity: EfuseIdentity,
    stall: rtc_cntl::StallKey,
    cache: CacheHandle,
    appcpu: dport::AppCoreHandle,
) -> Vec<(u32, u32, BoxedPeripheral)> {
    vec![
        (
            base::DPORT,
            accept::DPORT_LEN,
            Box::new(dport::DportView::new(cache.clone(), appcpu)) as BoxedPeripheral,
        ),
        (
            base::RTC_CNTL,
            rtc_cntl::RTC_CNTL_LEN,
            Box::new(rtc_cntl::RtcCntl::new(reset_cause, stall)),
        ),
        (
            base::APB_CTRL,
            accept::APB_CTRL_LEN,
            Box::new(accept::apb_ctrl(identity)),
        ),
        (base::TIMG0, timg::TIMG_LEN, Box::new(timg::Timg::timg0())),
        (
            base::I2C_ANA_MST,
            i2c_ana_mst::I2C_ANA_MST_LEN,
            Box::new(i2c_ana_mst::I2cAnaMst::new()),
        ),
        (base::TIMG1, timg::TIMG_LEN, Box::new(timg::Timg::timg1())),
        (base::GPIO, accept::GPIO_LEN, Box::new(accept::gpio())),
        (base::UART0, accept::UART0_LEN, Box::new(accept::uart0())),
        (base::IO_MUX, accept::IO_MUX_LEN, Box::new(accept::io_mux())),
        (base::SPI1, accept::SPI_LEN, Box::new(accept::spi("SPI1"))),
        (base::SPI0, accept::SPI_LEN, Box::new(accept::spi("SPI0"))),
        (
            base::EFUSE,
            efuse::EFUSE_LEN,
            Box::new(efuse::Efuse::new(identity)),
        ),
        // ROM-up, after the eFuse gate: `mmu_init` (`0x4000_95A4`) clears
        // both flash MMU tables, and `cache_flash_mmu_set` fills them. The
        // direct load never reaches them — P3's ledger has no entry — so
        // they go last, which is where the boot meets them.
        (
            memmap::FLASH_MMU_PRO,
            flash_mmu::LEN,
            Box::new(flash_mmu::FlashMmuView::new(cache)),
        ),
    ]
}
