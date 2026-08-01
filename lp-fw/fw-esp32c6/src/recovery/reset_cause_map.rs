//! Map ESP32-C6 SoC reset reasons to the platform-agnostic `ResetCause`.

use esp_hal::rtc_cntl::SocResetReason;
use lp_recovery::ResetCause;

/// Classification policy:
///
/// - USB-UART/JTAG resets are espflash / dev-tool resets — user-initiated,
///   never counted as crashes.
/// - Every watchdog flavor (RWDT, MWDT, super WDT) maps to `WatchdogReset`:
///   the eagerly-maintained frame stack is the blame record.
/// - Deep-sleep wake and anything unrecognized map to `Unknown`, which by
///   `lp-recovery` policy does NOT blame the code path (explicit crash
///   records are blamed regardless).
pub fn map_reset_cause(reason: Option<SocResetReason>) -> ResetCause {
    let Some(reason) = reason else {
        return ResetCause::Unknown;
    };
    match reason {
        SocResetReason::ChipPowerOn => ResetCause::PowerOn,
        SocResetReason::CoreUsbUart | SocResetReason::CoreUsbJtag | SocResetReason::Cpu0JtagCpu => {
            ResetCause::UserReset
        }
        SocResetReason::CoreSw | SocResetReason::Cpu0Sw => ResetCause::SoftwareReset,
        SocResetReason::CoreRtcWdt
        | SocResetReason::Cpu0RtcWdt
        | SocResetReason::SysRtcWdt
        | SocResetReason::SysSuperWdt
        | SocResetReason::CoreMwdt0
        | SocResetReason::CoreMwdt1
        | SocResetReason::Cpu0Mwdt0
        | SocResetReason::Cpu0Mwdt1 => ResetCause::WatchdogReset,
        SocResetReason::SysBrownOut => ResetCause::Brownout,
        _ => ResetCause::Unknown,
    }
}
