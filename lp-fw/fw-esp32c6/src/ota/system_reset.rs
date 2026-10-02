//! The reset every update step ends in.

/// A system reset, the way the ROM's `software_reset` does it: set
/// `LP_AON.sys_cfg.hpsys_sw_reset` (bit 31 of `0x600B_1034`). On silicon the
/// chip is gone before the next instruction. The spin is for the emulator,
/// which does not act on that bit yet
/// (`docs/defects/2026-09-29-the-emulated-c6-does-not-perform-a-software-reset.md`)
/// and reboots on the RTC watchdog a few seconds later — rather than falling
/// out of esp-hal's `-> !` wrapper into whatever code follows it.
pub fn system_reset() -> ! {
    const LP_AON_SYS_CFG: usize = 0x600B_1034;
    // SAFETY: the reset request register; nothing runs after it on silicon.
    unsafe {
        let v = core::ptr::read_volatile(LP_AON_SYS_CFG as *const u32);
        core::ptr::write_volatile(LP_AON_SYS_CFG as *mut u32, v | 1 << 31);
    }
    loop {
        core::hint::spin_loop();
    }
}
