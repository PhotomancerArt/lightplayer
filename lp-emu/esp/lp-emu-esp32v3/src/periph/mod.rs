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

use lp_emu_esp_common::periph::BoxedPeripheral;

use crate::loader::ResetCause;
use crate::memmap::periph as base;

/// The whole boot set, in [`crate::machine::PERIPHERAL_REGISTRATION_ORDER`].
///
/// `reset_cause` is what a direct load asserts (loader item 7).
pub fn boot_set(reset_cause: ResetCause) -> Vec<(u32, u32, BoxedPeripheral)> {
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
            Box::new(accept::apb_ctrl()),
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
        (base::EFUSE, accept::EFUSE_LEN, Box::new(accept::efuse())),
    ]
}
