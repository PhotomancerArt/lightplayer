//! Three small pieces of state the IDF leaves that are not clocks:
//!
//! - the three access-permission managers (`HP_APM`, `LP_APM0`, `LP_APM`)
//!   with their filters off (`func_ctrl = 0`), so no region check stands
//!   between the core and a peripheral;
//! - the crystal-frequency word in `LP_AON.store4` (`0x0028_0028`: 40 MHz,
//!   in both halves), which ROM routines read;
//! - the CPU's PC recorder on (`ASSIST_DEBUG.cpu0.rcd_en = 3`), which is
//!   what lets the ROM print `Saved PC:` after a reset — Studio's boot-loop
//!   detection reads that line.
//!
//! Measured (emulated IDF boot, 2026-10-17).

use super::mmio::{modify, write};

const HP_APM_FUNC_CTRL: u32 = 0x6009_90C4;
const LP_APM0_FUNC_CTRL: u32 = 0x6009_98C4;
const LP_APM_FUNC_CTRL: u32 = 0x600B_38C4;
const PCR_ASSIST_CONF: u32 = 0x6009_6100;
const ASSIST_DEBUG_RCD_EN: u32 = 0x600C_2044;
const LP_AON_STORE4: u32 = 0x600B_1010;
const XTAL_40MHZ_WORD: u32 = 0x0028_0028;

pub unsafe fn protection_and_pc_record() {
    // SAFETY: APM, PCR and ASSIST_DEBUG registers.
    unsafe {
        write(LP_APM_FUNC_CTRL, 0);
        write(LP_APM0_FUNC_CTRL, 0);
        write(HP_APM_FUNC_CTRL, 0);
        // The block's clock, then the recorder.
        modify(PCR_ASSIST_CONF, 0, 1);
        write(ASSIST_DEBUG_RCD_EN, 3);
        write(LP_AON_STORE4, XTAL_40MHZ_WORD);
    }
}
