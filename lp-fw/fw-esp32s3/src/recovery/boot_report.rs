//! Boot-time recovery init and the forensic record of the previous run.
//!
//! This runs as early as the heap allows and before anything crash-prone, so
//! that a crash from here on has somewhere to leave a breadcrumb. The report it
//! prints is the on-wire answer to "what died last time" — on the abort tier it
//! is the *only* answer, since nothing was caught in-process.

use alloc::boxed::Box;

use lp_recovery::BootAssessment;

use super::esp32s3_recovery_backend::Esp32S3RecoveryBackend;
use super::reset_cause_map::current_reset_cause;

/// Analyze the previous run, install the recovery global, and print the
/// assessment. Call once, after the heap allocator is live.
///
/// Returns the assessment so the caller can act on `safe_mode` — `main.rs` uses
/// it to skip auto-loading a project that has been crashing.
pub fn init_and_report() -> BootAssessment {
    let reset_cause = current_reset_cause();
    let (recovery, assessment) =
        lp_recovery::Recovery::init(Esp32S3RecoveryBackend::take(), reset_cause);
    // Leaked deliberately: `set_global` wants a `&'static mut`, and the
    // instance lives for the whole run by construction.
    lp_recovery::set_global(Box::leak(Box::new(recovery)));
    log_boot_assessment(&assessment);
    assessment
}

/// Print the boot assessment to serial. Uses `esp_println` because this runs
/// before the log crate has a sink.
pub fn log_boot_assessment(assessment: &BootAssessment) {
    esp_println::println!(
        "[RECOVERY] boot: cause={} level={} safe_mode={} prior_boot_complete={}",
        assessment.cause.as_str(),
        assessment.level.as_str(),
        assessment.safe_mode,
        assessment.prior_boot_complete,
    );
    let Some(crash) = &assessment.prior_crash else {
        return;
    };
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
    log_crash_pcs(crash.pc_count as usize, &crash.pc_frames);
}

/// Print the crash's captured PCs, or state that none were captured — using
/// the same wording discipline as the panic path: a target with no stack
/// walker must not report an *empty* stack. See `panic_path::print_frames`.
fn log_crash_pcs(count: usize, pcs: &[u32]) {
    let pcs = &pcs[..count.min(pcs.len())];
    if pcs.is_empty() {
        esp_println::println!(
            "[RECOVERY] no pcs recorded — the Xtensa walker ran and every candidate \
             failed its bounds checks, or the crash predates the walk"
        );
        return;
    }
    esp_println::print!("[RECOVERY] decode: just decode-backtrace-esp32s3");
    for pc in pcs {
        esp_println::print!(" 0x{pc:08x}");
    }
    esp_println::println!();
}
