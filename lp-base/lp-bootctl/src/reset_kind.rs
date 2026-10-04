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
