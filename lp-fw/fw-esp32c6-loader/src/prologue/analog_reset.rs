//! The brownout and analog-reset configuration (contract C6), as the IDF
//! leaves it — esp-hal never sets it, so the product has always run on the
//! bootloader's choice.
//!
//! Measured (emulated IDF boot, 2026-10-17), in order: `LP_ANA.fib_enable`
//! bit 2 cleared, then bit 1 cleared; `LP_ANA.bod_mode1_cntl` bit 31 set
//! (the brownout detector's mode-1 reset enabled). These are register
//! writes in `LP_ANA`'s own block — not the analog I2C bus the IDF's clock
//! bring-up uses.

use super::mmio::modify;

const LP_ANA: u32 = 0x600B_2C00;
const BOD_MODE1_CNTL: u32 = LP_ANA + 0x04;
const FIB_ENABLE: u32 = LP_ANA + 0x0c;
const BOD_MODE1_RESET_ENA: u32 = 1 << 31;

pub unsafe fn configure() {
    // SAFETY: LP_ANA registers.
    unsafe {
        modify(FIB_ENABLE, 1 << 2, 0);
        modify(FIB_ENABLE, 1 << 1, 0);
        modify(BOD_MODE1_CNTL, 0, BOD_MODE1_RESET_ENA);
    }
}
