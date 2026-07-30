//! Abort-tier crash recovery for the ESP32-S3.
//!
//! ⚠️ **Stub milestone.** M3 P3 writes this subsystem: the `lp-recovery`
//! backend over the RTC-fast-RAM region, the reset-cause map, the RWDT feeder,
//! and the panic handler that stages a crash record before resetting. The S3
//! is abort tier (ADR `2026-07-29-per-chip-fw-toolchains`), so the C6's
//! `catch_unwind` layer is a shape reference, not a source.
//!
//! P2 only lands the piece `serial::io_task` publishes into — see
//! [`watchdog::note_io_alive`].

pub mod watchdog;
