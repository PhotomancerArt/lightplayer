//! Map ESP32-S3 SoC reset reasons to the platform-agnostic `ResetCause`.
//!
//! ⚠️ **Not a copy of the C6 map.** `SocResetReason` is a per-chip enum in
//! esp-hal (`rtc_cntl/rtc/esp32s3.rs`), and the S3's variant set differs from
//! the C6's in three ways that matter:
//!
//! 1. **CPU-scoped resets are unprefixed.** The S3 is dual-core and ESP-IDF
//!    gives both cores the same reset codes, so esp-hal exposes `CpuSw`,
//!    `CpuMwdt0`, `CpuMwdt1`, `CpuRtcWdt` where the C6 has `Cpu0Sw`,
//!    `Cpu0Mwdt0`, and friends. Same meaning, different spelling.
//! 2. **There is no `Cpu0JtagCpu`.** The C6's "JTAG reset the CPU" code has no
//!    S3 counterpart in the enum, so it simply cannot appear here.
//! 3. **The S3 has three causes with no C6 equivalent** — `SysClkGlitch`,
//!    `CoreEfuseCrc`, `CorePwrGlitch` — see the classification notes below.
//!
//! Classification policy (unchanged from the C6, because it is the shared
//! `lp_recovery::ResetCause` vocabulary that fixes it):
//!
//! - USB-UART/JTAG resets are espflash / dev-tool resets — user-initiated,
//!   never counted as crashes.
//! - Every watchdog flavor (RWDT, MWDT, super WDT) maps to `WatchdogReset`:
//!   the eagerly-maintained frame stack is the blame record.
//! - Deep-sleep wake and anything unrecognized map to `Unknown`, which by
//!   `lp-recovery` policy does NOT blame the code path (explicit crash records
//!   are blamed regardless).

use esp_hal::rtc_cntl::SocResetReason;
use lp_recovery::ResetCause;

/// The mapped reset cause for the current boot.
pub fn current_reset_cause() -> ResetCause {
    map_reset_cause(esp_hal::system::reset_reason())
}

/// Translate the S3's SoC reset reason into the shared vocabulary.
///
/// The three S3-only causes are classified as follows, and each is a judgement
/// call worth knowing about when reading a boot report:
///
/// - `CorePwrGlitch` (glitch on the power rail) → [`ResetCause::Brownout`].
///   That variant is documented as "a power problem; the code path is not to
///   blame", which is exactly what a power glitch is. It is reported as
///   `brownout` even though the SoC distinguishes the two — the shared
///   vocabulary has no finer power category, and `Unknown` would lose the one
///   fact we actually know.
/// - `SysClkGlitch` (glitch on the clock) → [`ResetCause::Unknown`]. Not a
///   power fault and not the code path's fault; there is no honest slot for
///   it, and `Unknown` is the non-blaming default by design.
/// - `CoreEfuseCrc` (eFuse CRC error) → [`ResetCause::Unknown`]. A silicon /
///   provisioning fault. Same reasoning.
pub fn map_reset_cause(reason: Option<SocResetReason>) -> ResetCause {
    let Some(reason) = reason else {
        return ResetCause::Unknown;
    };
    match reason {
        SocResetReason::ChipPowerOn => ResetCause::PowerOn,
        SocResetReason::CoreUsbUart | SocResetReason::CoreUsbJtag => ResetCause::UserReset,
        SocResetReason::CoreSw | SocResetReason::CpuSw => ResetCause::SoftwareReset,
        SocResetReason::CoreRtcWdt
        | SocResetReason::CpuRtcWdt
        | SocResetReason::SysRtcWdt
        | SocResetReason::SysSuperWdt
        | SocResetReason::CoreMwdt0
        | SocResetReason::CoreMwdt1
        | SocResetReason::CpuMwdt0
        | SocResetReason::CpuMwdt1 => ResetCause::WatchdogReset,
        // Power faults: brownout proper, and the S3-only supply glitch that
        // has no separate slot in the shared vocabulary.
        SocResetReason::SysBrownOut | SocResetReason::CorePwrGlitch => ResetCause::Brownout,
        // Deep-sleep wake, plus the two S3-only hardware faults with no honest
        // equivalent (`SysClkGlitch`, `CoreEfuseCrc`). `Unknown` does not blame
        // the code path, which is the right default for all three.
        SocResetReason::CoreDeepSleep
        | SocResetReason::SysClkGlitch
        | SocResetReason::CoreEfuseCrc => ResetCause::Unknown,
    }
}
