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
pub mod efuse;

use lp_emu_esp_common::periph::BoxedPeripheral;

use crate::loader::{EfuseIdentity, ResetCause};
use crate::memmap::periph as base;

/// The whole boot set, in [`crate::machine::PERIPHERAL_REGISTRATION_ORDER`].
///
/// `reset_cause` is what a direct load asserts (loader item 7); `identity`
/// is the part this run claims to be (MAC and chip revision), which reaches
/// two blocks — the eFuse view and, for the revision's top bit, `APB_CTRL`.
pub fn boot_set(
    reset_cause: ResetCause,
    identity: EfuseIdentity,
) -> Vec<(u32, u32, BoxedPeripheral)> {
    vec![
        (
            base::DPORT,
            accept::DPORT_LEN,
            Box::new(accept::dport()) as BoxedPeripheral,
        ),
        (
            base::RTC_CNTL,
            accept::RTC_CNTL_LEN,
            Box::new(accept::rtc_cntl(reset_cause)),
        ),
        (
            base::APB_CTRL,
            accept::APB_CTRL_LEN,
            Box::new(accept::apb_ctrl(identity)),
        ),
        (
            base::TIMG0,
            accept::TIMG_LEN,
            Box::new(accept::timg("TIMG0")),
        ),
        (
            base::I2C_ANA_MST,
            accept::I2C_ANA_MST_LEN,
            Box::new(accept::i2c_ana_mst()),
        ),
        (
            base::TIMG1,
            accept::TIMG_LEN,
            Box::new(accept::timg("TIMG1")),
        ),
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
    ]
}
