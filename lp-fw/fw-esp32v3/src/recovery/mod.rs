//! Abort-tier crash recovery for the classic ESP32.
//!
//! The classic is **abort tier** per ADR `2026-07-29-per-chip-fw-toolchains`,
//! the same tier as fw-esp32s3: `panic = "abort"` plus the `lp-recovery` RTC
//! ledger. A panic is terminal for the boot; all this subsystem can do is make
//! the *next* boot able to say what died.
//!
//! ## Why this exists on this chip in particular
//!
//! On the S3 the ledger is a convenience — its panic handler can print, and
//! usually does. On the classic it is the **only** diagnostic channel that
//! works for the fault class this bring-up is stuck on.
//!
//! Measured 2026-08-01 (`docs/defects/2026-08-01-classic-rmt-open-fault.md`): a
//! fault raised while WS281x channels are opening resets the chip in well under
//! a millisecond, no matter what the panic handler does. Masking interrupts,
//! draining the TX FIFO first, printing the line before the path, emitting the
//! path in 4-byte chunks, and trading 46 KB of heap for stack all yielded the
//! same ~5 characters. That is the signature of a second exception taken inside
//! exception context, which vectors straight to reset and cannot be out-run
//! from Rust. Printing faster is not a fix; **not needing to print** is. A
//! record staged into RTC RAM survives the reset and is read back on the next
//! boot, when the chip is healthy and the UART works.
//!
//! ## What is here, and what is not
//!
//! - [`esp32v3_recovery_backend`]: the persistent region in RTC fast RAM and
//!   the software-reset hook.
//! - [`reset_cause_map`]: the classic's `SocResetReason` → platform-agnostic
//!   `ResetCause`. Deliberately not the S3's map; the variant sets differ.
//! - [`panic_path`]: print if it can, stage a breadcrumb regardless, reset.
//! - [`boot_report`]: boot-time init and the "what died last run" report.
//!
//! **No watchdog module.** fw-esp32s3 pairs the ledger with an RWDT and an
//! io-task-aware feed policy; that stays M7 work here. Two reasons, and the
//! second is the load-bearing one: the RWDT needs the io-task feed contract
//! wired before it is armed or it resets the board every `BOOT_TIMEOUT_MS`
//! forever, and arming a watchdog on a board that is *currently under
//! investigation for reset-looping* would add a second reset source to a
//! diagnosis whose whole difficulty is attributing the first one.
//!
//! `lp-recovery`'s incomplete-boot counter still works without it — that
//! counter is driven by `mark_boot_complete`, not by the watchdog — so safe
//! mode still breaks a boot loop.

pub mod boot_report;
pub mod esp32v3_recovery_backend;
pub mod panic_path;
pub mod reset_cause_map;
