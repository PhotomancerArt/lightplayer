//! Map classic-ESP32 SoC reset reasons to the platform-agnostic `ResetCause`.
//!
//! ⚠️ **Not a copy of the S3 map.** `SocResetReason` is a per-chip enum in
//! esp-hal (`rtc_cntl/rtc/esp32.rs`), and the classic's variant set differs from
//! the S3's in four ways that matter:
//!
//! 1. **There are no USB reset causes at all.** The classic has no
//!    USB-Serial-JTAG peripheral, so `CoreUsbUart` / `CoreUsbJtag` — the S3's
//!    entire [`ResetCause::UserReset`] arm — simply do not exist here. The
//!    consequence is not cosmetic: on this board espflash resets the chip by
//!    toggling EN through the CH340K's DTR/RTS lines, which is an external
//!    *chip* reset and reports as `ChipPowerOn`. **A dev-tool reset is
//!    therefore indistinguishable from a power-up on this chip**, and both are
//!    reported as `power-on`. That is honest — the silicon really does not
//!    distinguish them — but it means "power-on" here does not imply the board
//!    lost power, and it is why no variant maps to `UserReset`.
//! 2. **CPU-scoped resets are spelled inconsistently.** esp-hal exposes
//!    `CpuMwdt0`, `Cpu0Sw`, `Cpu0RtcWdt` (note: `CpuMwdt0` unprefixed but
//!    `Cpu0Sw` prefixed), because ESP-IDF gives both cores the same codes and
//!    the names were transcribed as-is.
//! 3. **`CoreSdio` is classic-only** — the SDIO slave peripheral can reset the
//!    digital core. Nothing in this firmware uses SDIO.
//! 4. **`Cpu1Cpu0` is classic-only** — PRO_CPU resetting APP_CPU via
//!    `DPORT_APPCPU_RESETTING`.
//!
//! Classification policy (unchanged from the S3 and the C6, because it is the
//! shared `lp_recovery::ResetCause` vocabulary that fixes it):
//!
//! - Every watchdog flavor (RWDT, MWDT) maps to `WatchdogReset`: the eagerly
//!   maintained frame stack is the blame record.
//! - Deep-sleep wake and anything unrecognized map to `Unknown`, which by
//!   `lp-recovery` policy does NOT blame the code path (explicit crash records
//!   are blamed regardless).

use esp_hal::rtc_cntl::SocResetReason;
use lp_recovery::ResetCause;

/// The mapped reset cause for the current boot.
pub fn current_reset_cause() -> ResetCause {
    map_reset_cause(esp_hal::system::reset_reason())
}

/// Translate the classic's SoC reset reason into the shared vocabulary.
///
/// The two classic-only causes are classified as follows, and each is a
/// judgement call worth knowing about when reading a boot report:
///
/// - `Cpu1Cpu0` (PRO_CPU reset APP_CPU) → [`ResetCause::SoftwareReset`]. It is
///   literally software resetting a CPU, which is what that variant means. It
///   should never appear in this firmware — the app runs on PRO_CPU and
///   APP_CPU is never started — so seeing it at all is the interesting signal,
///   and `SoftwareReset` says "something deliberately did this" rather than
///   burying it in `Unknown`.
/// - `CoreSdio` (the SDIO slave reset the digital core) → [`ResetCause::Unknown`].
///   Nothing here uses SDIO, there is no honest slot for it in the shared
///   vocabulary, and `Unknown` is the non-blaming default by design.
pub fn map_reset_cause(reason: Option<SocResetReason>) -> ResetCause {
    let Some(reason) = reason else {
        return ResetCause::Unknown;
    };
    match reason {
        // Also what an espflash EN-line reset looks like on this board — see
        // the module docs. There is no `UserReset` arm on this chip.
        SocResetReason::ChipPowerOn => ResetCause::PowerOn,
        SocResetReason::CoreSw | SocResetReason::Cpu0Sw | SocResetReason::Cpu1Cpu0 => {
            ResetCause::SoftwareReset
        }
        SocResetReason::CoreRtcWdt
        | SocResetReason::Cpu0RtcWdt
        | SocResetReason::SysRtcWdt
        | SocResetReason::CoreMwdt0
        | SocResetReason::CoreMwdt1
        | SocResetReason::CpuMwdt0 => ResetCause::WatchdogReset,
        SocResetReason::SysBrownOut => ResetCause::Brownout,
        // Deep-sleep wake, plus the classic-only SDIO reset with no honest
        // equivalent. `Unknown` does not blame the code path, which is the
        // right default for both.
        SocResetReason::CoreDeepSleep | SocResetReason::CoreSdio => ResetCause::Unknown,
    }
}
