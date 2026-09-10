//! The ESP32-C6 LP analog I2C clock: what a fresh board's factory firmware
//! leaves gated, and what our bootloader cannot boot without.
//!
//! ESP-IDF apps gate the LP peripheral clocks they do not use at startup,
//! `LPPERI_CLK_EN` bit 29 (`LP_ANA_I2C_CK_EN`) among them. They never miss
//! it: an ESP-IDF app drives the analog bus through the HP aperture. The
//! second-stage bootloader inside our merged image (espflash 3.3.0's bundled
//! ESP-IDF v5.1-beta1) drives it through the **LP** aperture
//! (`LP_I2C_ANA_MST`, `0x600b2400`), and every reset a flasher can send —
//! USB-Serial-JTAG or the ROM's flash-boot watchdog — is HP-only, so the
//! gated clock survives into the boot of the firmware we just wrote. The
//! bootloader's first regi2c write latches busy and it spins at
//! `0x4086ed7a` forever; only a power-on reset (a replug) used to fix it.
//!
//! Bench-proven on 2026-09-06 (Seeed XIAO ESP32C6 `A0:F2:62:85:A8:7C`, both
//! directions — clearing the bit reproduces the hang on demand): set bit 29
//! **and pulse `LPPERI_RESET_EN` bit 29** over the ROM/stub before the
//! closing reset, and the ordinary reset boots LightPlayer. Setting the
//! clock alone does NOT clear the latched busy.
//!
//! The register plan is pure so it can be tested; the IO lives in
//! `host_esp32_flash::restore_lp_analog_i2c_clock`, and its browser twin is
//! `restoreLpAnalogI2cClock` in `browser_esp32_flash.js` (which has no
//! tests — keep the two in step by hand). See
//! `docs/defects/2026-09-06-c6-first-flash-bootloader-hang-lp-analog-i2c-clock.md`.

/// `LPPERI_CLK_EN`: LP peripheral clock gates. Bit 29 = `LP_ANA_I2C_CK_EN`
/// (power-on 1).
pub(super) const LPPERI_CLK_EN: u32 = 0x600B_2800;
/// `LPPERI_RESET_EN`: LP peripheral reset lines. Bit 29 =
/// `LP_ANA_I2C_RESET_EN` (power-on 0).
pub(super) const LPPERI_RESET_EN: u32 = 0x600B_2804;
/// The LP analog I2C master's bit in both registers above.
pub(super) const LP_ANA_I2C_BIT: u32 = 1 << 29;
/// `LP_I2C_ANA_MST_I2C0_CTRL`: the master's transaction register; bit 25 is
/// busy, and it is what the bootloader busy-waits on.
pub(super) const LP_I2C_ANA_MST_I2C0_CTRL: u32 = 0x600B_2400;
pub(super) const LP_I2C_ANA_MST_BUSY: u32 = 1 << 25;

/// What the registers said the board needs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct LpAnalogI2cFix {
    /// `LPPERI_CLK_EN` bit 29 read clear.
    pub clock_was_gated: bool,
    /// The master read busy — a transaction latched while unclocked.
    pub master_was_busy: bool,
}

/// Decide from the two reads whether the sequence must run. `None` means the
/// board is clean: nothing to write, nothing to log.
pub(super) fn lp_analog_i2c_plan(clk_en: u32, ctrl: u32) -> Option<LpAnalogI2cFix> {
    let fix = LpAnalogI2cFix {
        clock_was_gated: clk_en & LP_ANA_I2C_BIT == 0,
        master_was_busy: ctrl & LP_I2C_ANA_MST_BUSY != 0,
    };
    (fix.clock_was_gated || fix.master_was_busy).then_some(fix)
}

/// The one flash-log line that tells the story. Both providers emit the same
/// words so a log reads the same whichever wrote the board.
pub(super) fn describe(
    fix: LpAnalogI2cFix,
    clk_before: u32,
    clk_after: u32,
    busy_after: bool,
) -> String {
    let what = match (fix.clock_was_gated, fix.master_was_busy) {
        (true, _) => "restored the LP analog I2C clock the previous firmware left gated",
        (false, true) => "reset the LP analog I2C master the previous firmware left busy",
        (false, false) => unreachable!("describe is only called with a plan"),
    };
    let busy = if busy_after {
        "busy still set; the board may need a replug"
    } else {
        "busy cleared"
    };
    format!("{what} (LPPERI_CLK_EN 0x{clk_before:08x} -> 0x{clk_after:08x}, {busy})")
}

#[cfg(test)]
mod tests {
    use super::*;

    const POWER_ON_CLK_EN: u32 = 0x7f00_0000;
    /// Read from the stuck XIAO on 2026-09-06.
    const FACTORY_GATED_CLK_EN: u32 = 0x4100_0000;
    const STUCK_CTRL: u32 = 0x0200_0e6d;

    #[test]
    fn a_clean_board_needs_nothing() {
        assert_eq!(lp_analog_i2c_plan(POWER_ON_CLK_EN, 0), None);
    }

    #[test]
    fn the_factory_state_needs_the_clock_and_the_reset() {
        assert_eq!(
            lp_analog_i2c_plan(FACTORY_GATED_CLK_EN, STUCK_CTRL),
            Some(LpAnalogI2cFix {
                clock_was_gated: true,
                master_was_busy: true,
            })
        );
    }

    #[test]
    fn a_gated_clock_with_an_idle_master_still_needs_the_fix() {
        // Before our bootloader ever ran: nothing latched yet, but the boot
        // after the reset would latch it.
        assert_eq!(
            lp_analog_i2c_plan(FACTORY_GATED_CLK_EN, 0),
            Some(LpAnalogI2cFix {
                clock_was_gated: true,
                master_was_busy: false,
            })
        );
    }

    #[test]
    fn a_busy_master_with_the_clock_on_needs_the_reset() {
        // Someone set the clock without pulsing the reset (the bench showed
        // the clock alone does not clear busy).
        assert_eq!(
            lp_analog_i2c_plan(POWER_ON_CLK_EN, STUCK_CTRL),
            Some(LpAnalogI2cFix {
                clock_was_gated: false,
                master_was_busy: true,
            })
        );
    }

    #[test]
    fn the_log_line_names_the_registers() {
        let fix = lp_analog_i2c_plan(FACTORY_GATED_CLK_EN, STUCK_CTRL).unwrap();
        assert_eq!(
            describe(fix, FACTORY_GATED_CLK_EN, 0x6100_0000, false),
            "restored the LP analog I2C clock the previous firmware left gated \
             (LPPERI_CLK_EN 0x41000000 -> 0x61000000, busy cleared)"
        );
        let fix = lp_analog_i2c_plan(POWER_ON_CLK_EN, STUCK_CTRL).unwrap();
        assert_eq!(
            describe(fix, POWER_ON_CLK_EN, POWER_ON_CLK_EN, true),
            "reset the LP analog I2C master the previous firmware left busy \
             (LPPERI_CLK_EN 0x7f000000 -> 0x7f000000, busy still set; the board may need a replug)"
        );
    }
}
