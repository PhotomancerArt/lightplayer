//! ESP32 glue for the `lp-recovery` crash-recovery system.
//!
//! - [`esp32_recovery_backend`]: the persistent region in RTC fast RAM and
//!   the software-reset hook.
//! - [`reset_cause_map`]: `SocResetReason` → platform-agnostic `ResetCause`.
//! - [`watchdog`]: RWDT arming and the io-task-aware feed policy.

pub mod esp32_recovery_backend;
pub mod reset_cause_map;
pub mod watchdog;

pub use esp32_recovery_backend::Esp32RecoveryBackend;

use lp_recovery::BootAssessment;

/// The mapped reset cause for the current boot.
pub fn current_reset_cause() -> lp_recovery::ResetCause {
    reset_cause_map::map_reset_cause(esp_hal::system::reset_reason())
}

/// Print the boot assessment to serial — the on-wire forensic record of
/// what happened last run. Uses esp_println (works before logger init).
pub fn log_boot_assessment(assessment: &BootAssessment) {
    esp_println::println!(
        "[RECOVERY] boot: cause={} level={} safe_mode={} prior_boot_complete={}",
        assessment.cause.as_str(),
        assessment.level.as_str(),
        assessment.safe_mode,
        assessment.prior_boot_complete,
    );
    if let Some(crash) = &assessment.prior_crash {
        esp_println::println!(
            "[RECOVERY] last run crashed ({}): at {}: {}",
            crash.cause.as_str(),
            crash.path_display(),
            crash.msg.as_str(),
        );
        if crash.cause == lp_recovery::CrashCause::Oom {
            let heap = crash.heap;
            esp_println::println!(
                "[RECOVERY] oom stats: requested={} align={} free={} used={}",
                heap.requested,
                heap.align,
                heap.free,
                heap.used,
            );
        }
    }
}
