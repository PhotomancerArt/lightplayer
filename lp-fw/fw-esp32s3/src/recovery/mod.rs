//! Abort-tier crash recovery for the ESP32-S3.
//!
//! The S3 is **abort tier** per ADR `2026-07-29-per-chip-fw-toolchains`:
//! `panic = "abort"` plus the `lp-recovery` RTC ledger. A panic is terminal for
//! the boot; all this subsystem can do is make the *next* boot able to say what
//! died. `fw-esp32c6/src/recovery/` is a shape reference for the file split,
//! not a source — its `catch_unwind` layer has no counterpart here.
//!
//! - [`esp32s3_recovery_backend`]: the persistent region in RTC fast RAM and
//!   the software-reset hook.
//! - [`reset_cause_map`]: the S3's `SocResetReason` → platform-agnostic
//!   `ResetCause`. Deliberately not the C6's map; the variant sets differ.
//! - [`panic_path`]: print, stage a breadcrumb, reset. Read its module docs for
//!   why there is no `is_esp_sync_reentrant_lock_panic` guard here.
//! - [`boot_report`]: boot-time init and the "what died last run" report.
//! - [`watchdog`]: RWDT arming and the io-task-aware feed policy. Armed by
//!   `main.rs` next to the `io_task` spawn that feeds it — see that module's
//!   docs for why the two must stay together.

pub mod boot_report;
pub mod esp32s3_recovery_backend;
pub mod panic_path;
pub mod reset_cause_map;
pub mod watchdog;
