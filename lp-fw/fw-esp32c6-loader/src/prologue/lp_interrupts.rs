//! The LP domain's interrupts, masked and cleared as the IDF leaves them.
//!
//! LP registers survive an HP-only reset, so a previous app's enabled LP
//! interrupt (its RWDT stage-0 interrupt, an LP timer alarm, a brownout
//! interrupt) can still be enabled, or pending, when this stage runs.
//! Measured (emulated IDF boot, 2026-10-17): `int_ena = 0` in `LP_WDT`,
//! `LP_TIMER`, `LP_ANA` and `PMU`; then the super watchdog's and the RWDT's
//! (bits 30, 31), the LP timer's and the brownout's (bit 31) cleared.

use super::mmio::write;

const LP_WDT_INT_ENA: u32 = 0x600B_1C2C;
const LP_WDT_INT_CLR: u32 = 0x600B_1C30;
const LP_TIMER_INT_ENA: u32 = 0x600B_0C40;
const LP_TIMER_INT_CLR: u32 = 0x600B_0C44;
const LP_ANA_INT_ENA: u32 = 0x600B_2C28;
const LP_ANA_INT_CLR: u32 = 0x600B_2C2C;
const PMU_INT_ENA: u32 = 0x600B_0164;

pub unsafe fn mask_and_clear() {
    // SAFETY: LP peripheral interrupt registers.
    unsafe {
        write(LP_WDT_INT_ENA, 0);
        write(LP_TIMER_INT_ENA, 0);
        write(LP_ANA_INT_ENA, 0);
        write(PMU_INT_ENA, 0);
        write(LP_WDT_INT_CLR, 1 << 30);
        write(LP_TIMER_INT_CLR, 1 << 31);
        write(LP_ANA_INT_CLR, 1 << 31);
        write(LP_WDT_INT_CLR, 1 << 31);
    }
}
