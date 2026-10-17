//! The three watchdogs, left as the IDF leaves them (contract C4): a core
//! that hangs before `esp_hal::init` must still die **warm**, so a failing
//! trial rolls back.
//!
//! - TIMG0's MWDT is armed by the chip at power-on with *flash-boot
//!   protection*; the ROM never touches it. Left alone it resets the chip a
//!   few hundred ms in. Off, as the IDF turns it off.
//! - The RTC watchdog is (re-)armed for ~9 s with a system reset. Re-armed,
//!   not only armed: a previous app's RWDT survives an HP-only reset on
//!   silicon.
//! - The super watchdog is set to feed itself.
//!
//! Measured values (emulated IDF boot, 2026-10-17): RWDT `wdtconfig0`
//! `0x0007_EA00` then `0xC007_EA00` (enabled, stage 0 = system reset),
//! `wdtconfig1` `0x0009_56A0` (612,000 ticks: 9 s at RC_SLOW/2); TIMG0
//! `wdtconfig0` bit 14 (flash-boot) cleared, then bit 22 (config update);
//! SWD `swd_conf` bit 18 (auto-feed) set. Each block's write-protect key is
//! `0x50D8_3AA1`.

use super::mmio::{modify, write};

const WDT_WKEY: u32 = 0x50D8_3AA1;

const TIMG0: u32 = 0x6000_8000;
const TIMG0_WDTCONFIG0: u32 = TIMG0 + 0x48;
const TIMG0_WDTWPROTECT: u32 = TIMG0 + 0x64;
const MWDT_FLASHBOOT_MOD_EN: u32 = 1 << 14;
const MWDT_CONF_UPDATE_EN: u32 = 1 << 22;

const LP_WDT: u32 = 0x600B_1C00;
const RWDT_CONFIG0: u32 = LP_WDT;
const RWDT_CONFIG1: u32 = LP_WDT + 0x04;
const RWDT_FEED: u32 = LP_WDT + 0x14;
const RWDT_WPROTECT: u32 = LP_WDT + 0x18;
const SWD_CONF: u32 = LP_WDT + 0x1c;
const SWD_WPROTECT: u32 = LP_WDT + 0x20;
const LP_WDT_INT_CLR: u32 = LP_WDT + 0x30;

/// Stage lengths and the stage-0 action, disabled.
const RWDT_CONFIG0_OFF: u32 = 0x0007_EA00;
/// The same, enabled.
const RWDT_CONFIG0_ON: u32 = 0xC007_EA00;
/// Stage 0's hold: ~9 s.
const RWDT_STAGE0_HOLD: u32 = 0x0009_56A0;
const RWDT_FEED_BIT: u32 = 1 << 31;
const RWDT_INT_CLR: u32 = 1 << 31;
const SWD_AUTO_FEED_EN: u32 = 1 << 18;

pub unsafe fn super_watchdog_auto_feed() {
    // SAFETY: LP_WDT's SWD registers, behind their key.
    unsafe {
        write(SWD_WPROTECT, WDT_WKEY);
        modify(SWD_CONF, 0, SWD_AUTO_FEED_EN);
        write(SWD_WPROTECT, 0);
    }
}

pub unsafe fn rtc_watchdog_9s() {
    // SAFETY: LP_WDT's RWDT registers, behind their key.
    unsafe {
        write(RWDT_WPROTECT, WDT_WKEY);
        write(RWDT_CONFIG0, RWDT_CONFIG0_OFF);
        write(LP_WDT_INT_CLR, RWDT_INT_CLR);
        write(RWDT_CONFIG1, RWDT_STAGE0_HOLD);
        write(RWDT_FEED, RWDT_FEED_BIT);
        write(RWDT_CONFIG0, RWDT_CONFIG0_ON);
        write(RWDT_WPROTECT, 0);
    }
}

pub unsafe fn flash_boot_protection_off() {
    // SAFETY: TIMG0's MWDT registers, behind their key.
    unsafe {
        write(TIMG0_WDTWPROTECT, WDT_WKEY);
        modify(TIMG0_WDTCONFIG0, MWDT_FLASHBOOT_MOD_EN, 0);
        modify(TIMG0_WDTCONFIG0, 0, MWDT_CONF_UPDATE_EN);
        write(TIMG0_WDTWPROTECT, 0);
    }
}
