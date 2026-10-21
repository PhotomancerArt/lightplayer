//! Cold or warm: what the reset before this boot says about a trial core.
//!
//! The loader and the core both classify the reset the same way — through
//! this one table — so they make the same [`crate::choose`].
//!
//! - **Cold**: the power went away (power-on, brownout), the chip woke from
//!   a deep sleep it was put into, or an **external host** reset it (the
//!   USB-Serial-JTAG bridge's chip reset that a flasher or Studio drives, a
//!   JTAG debugger, an SDIO host). None of these is evidence that the core
//!   that ran before failed: a host resetting a board that waits on a trial
//!   must not roll a good build back.
//! - **Warm**: the chip reset itself — a panic's software reset, any
//!   watchdog, an eFuse CRC fault. A trial core that dies this way failed.
//! - **Anything else** (an unknown or reserved code) is warm. That errs
//!   towards rollback, which is how a failed trial was treated before any
//!   cold rule existed.
//!
//! # The codes
//!
//! The ESP32-C6's reset-reason codes are the values the mask ROM's
//! `rtc_get_reset_reason(0)` returns. Their names are read out of the
//! ROM's own 25-entry name table (`0x4004_a8e8` in the vendored
//! `lp-emu/esp/roms/esp32c6_rev0_rom.elf`; transcribed in
//! `lp-emu/esp/lp-emu-esp32c6/src/loader.rs`), and they agree with the
//! reset-source descriptions in esp-hal's `SocResetReason` for the C6
//! (`third_party/esp-hal/src/rtc_cntl/rtc/esp32c6.rs`). The Technical
//! Reference Manual's reset-sources table (chapter "Reset and Clock") was
//! not available to the author of this table; the ROM is the primary
//! source it cites.
//!
//! ```text
//! 0x01 POWERON         cold   power-on
//! 0x03 LP_SW_HPSYS     warm   software reset of the HP system
//! 0x05 SLEEP_WAKEUP    cold   wake from deep sleep (the power button's off)
//! 0x06 SDIO_HPSYS      cold   an SDIO host reset the core
//! 0x07 TG0_WDT_HPSYS   warm   timer group 0 watchdog
//! 0x08 TG1_WDT_HPSYS   warm   timer group 1 watchdog
//! 0x09 LP_WDT_HPSYS    warm   RTC watchdog, core
//! 0x0B TG0_WDT_CPU     warm   timer group 0 watchdog, CPU
//! 0x0C SW_CPU          warm   software reset of the CPU
//! 0x0D LP_WDT_CPU      warm   RTC watchdog, CPU
//! 0x0F LP_BOD_SYS      cold   brownout
//! 0x10 LP_WDT_SYS      warm   RTC watchdog, system
//! 0x11 TG1_WDT_CPU     warm   timer group 1 watchdog, CPU
//! 0x12 LP_SWDT_SYS     warm   super watchdog
//! 0x14 EFUSE_HPSYS     warm   eFuse CRC error
//! 0x15 USB_UART_HPSYS  cold   the USB-Serial-JTAG bridge's chip reset (host)
//! 0x16 USB_JTAG_HPSYS  cold   the USB-Serial-JTAG's JTAG reset (host)
//! 0x18 JTAG_CPU        cold   a JTAG debugger reset the CPU (host)
//! ```
//!
//! # The ESP32-S3's codes
//!
//! The same classes, code by code, for the values the S3's mask ROM's
//! `rtc_get_reset_reason(0)` returns (ROM entry `0x4000_057c`). Their names
//! are the ROM's own 24-entry name table (`0x3FF1_8E10` in the vendored
//! `lp-emu/esp/roms/esp32s3_rev0_rom.elf`'s `.rodata`, strings from
//! `0x3FF1_A971`; read 2026-10-21 by following the pointers to `POWERON`),
//! and they agree with esp-hal's `SocResetReason` for the S3
//! (`third_party/esp-hal/src/rtc_cntl/rtc/esp32s3.rs`) code for code, except
//! `0x0A`, which esp-hal does not name.
//!
//! ```text
//! 0x01 POWERON              cold   power-on
//! 0x03 RTC_SW_SYS_RST       warm   software reset of the digital core
//! 0x05 DSLEEP               cold   wake from deep sleep
//! 0x07 TG0WDT_SYS_RST       warm   timer group 0 watchdog
//! 0x08 TG1WDT_SYS_RST       warm   timer group 1 watchdog
//! 0x09 RTCWDT_SYS_RST       warm   RTC watchdog, digital core
//! 0x0A INTRUSION_RST        warm*  not in esp-hal; no description
//! 0x0B TG0WDT_CPU_RST       warm   timer group 0 watchdog, CPU
//! 0x0C RTC_SW_CPU_RST       warm   software reset of the CPU
//! 0x0D RTCWDT_CPU_RST       warm   RTC watchdog, CPU
//! 0x0F BROWNOUT_RST         cold   brownout
//! 0x10 RTCWDT_RTC_RST       warm   RTC watchdog, digital core and RTC
//! 0x11 TG1WDT_CPU_RST       warm   timer group 1 watchdog, CPU
//! 0x12 SUPER_WDT_RST        warm   super watchdog
//! 0x13 GLITCH_RTC_RST       warm*  a glitch on the clock (esp-hal `SysClkGlitch`)
//! 0x14 EFUSE_RST            warm   eFuse CRC error
//! 0x15 USB_UART_CHIP_RESET  cold   the USB-Serial-JTAG bridge's chip reset (host)
//! 0x16 USB_JTAG_CHIP_RESET  cold   the USB-Serial-JTAG's JTAG reset (host)
//! 0x17 POWER_GLITCH_RESET   warm*  a glitch on the power (esp-hal `CorePwrGlitch`)
//! ```
//!
//! `warm*`: the codes the C6's table has no counterpart for, which take the
//! unknown rule (warm) until someone rules otherwise. The S3 has no SDIO
//! host reset and no `JTAG_CPU` code.

/// What a reset says about the boot before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetKind {
    Cold,
    Warm,
}

impl ResetKind {
    /// Classify an ESP32-C6 reset-reason code (`rtc_get_reset_reason(0)`).
    pub const fn from_c6_reason(code: u32) -> Self {
        match code {
            0x01 | 0x05 | 0x06 | 0x0F | 0x15 | 0x16 | 0x18 => Self::Cold,
            _ => Self::Warm,
        }
    }

    /// Classify an ESP32-S3 reset-reason code (`rtc_get_reset_reason(0)`).
    pub const fn from_s3_reason(code: u32) -> Self {
        match code {
            0x01 | 0x05 | 0x0F | 0x15 | 0x16 => Self::Cold,
            _ => Self::Warm,
        }
    }

    pub const fn is_cold(self) -> bool {
        matches!(self, Self::Cold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_and_brownout_are_cold() {
        assert_eq!(ResetKind::from_c6_reason(0x01), ResetKind::Cold);
        assert_eq!(ResetKind::from_c6_reason(0x0F), ResetKind::Cold);
    }

    #[test]
    fn a_host_reset_is_cold() {
        for code in [0x15, 0x16, 0x18, 0x06] {
            assert_eq!(
                ResetKind::from_c6_reason(code),
                ResetKind::Cold,
                "{code:#x}"
            );
        }
    }

    #[test]
    fn the_chip_resetting_itself_is_warm() {
        for code in [
            0x03, 0x07, 0x08, 0x09, 0x0B, 0x0C, 0x0D, 0x10, 0x11, 0x12, 0x14,
        ] {
            assert_eq!(
                ResetKind::from_c6_reason(code),
                ResetKind::Warm,
                "{code:#x}"
            );
        }
    }

    #[test]
    fn s3_power_brownout_and_deep_sleep_are_cold() {
        for code in [0x01, 0x05, 0x0F] {
            assert_eq!(
                ResetKind::from_s3_reason(code),
                ResetKind::Cold,
                "{code:#x}"
            );
        }
    }

    #[test]
    fn s3_a_host_reset_is_cold() {
        for code in [0x15, 0x16] {
            assert_eq!(
                ResetKind::from_s3_reason(code),
                ResetKind::Cold,
                "{code:#x}"
            );
        }
    }

    #[test]
    fn s3_the_chip_resetting_itself_is_warm() {
        for code in [
            0x03, 0x07, 0x08, 0x09, 0x0B, 0x0C, 0x0D, 0x10, 0x11, 0x12, 0x14,
        ] {
            assert_eq!(
                ResetKind::from_s3_reason(code),
                ResetKind::Warm,
                "{code:#x}"
            );
        }
    }

    /// The three codes with no C6 counterpart take the unknown rule.
    #[test]
    fn s3_intrusion_and_glitches_are_warm() {
        for code in [0x0A, 0x13, 0x17] {
            assert_eq!(
                ResetKind::from_s3_reason(code),
                ResetKind::Warm,
                "{code:#x}"
            );
        }
    }

    #[test]
    fn s3_unknown_codes_are_warm() {
        for code in [0x00, 0x02, 0x04, 0x06, 0x0E, 0x18, 0x19, 0x1F, 0xFF] {
            assert_eq!(
                ResetKind::from_s3_reason(code),
                ResetKind::Warm,
                "{code:#x}"
            );
        }
    }

    /// Every code `0x00..=0x1F`, pinned: a change to the table is a change
    /// to this list.
    #[test]
    fn s3_every_code_is_pinned() {
        let cold = [0x01, 0x05, 0x0F, 0x15, 0x16];
        for code in 0x00..=0x1F {
            assert_eq!(
                ResetKind::from_s3_reason(code).is_cold(),
                cold.contains(&code),
                "{code:#x}"
            );
        }
    }

    #[test]
    fn unknown_codes_are_warm() {
        for code in [0x00, 0x02, 0x04, 0x0A, 0x0E, 0x13, 0x17, 0x19, 0x1F, 0xFF] {
            assert_eq!(
                ResetKind::from_c6_reason(code),
                ResetKind::Warm,
                "{code:#x}"
            );
        }
    }
}
