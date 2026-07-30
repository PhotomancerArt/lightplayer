//! I/O liveness signal for the RTC watchdog feeder.
//!
//! ⚠️ **Partial by design.** M3 P3 owns the abort-tier recovery subsystem —
//! the RWDT arming and the `WatchdogFeeder` that consumes this flag. Only the
//! *producer* side lives here, because `serial::io_task` publishes it on every
//! loop iteration and the serial port (P2) cannot be verbatim without it.
//!
//! Feed policy, for context: the server loop feeds the RWDT every frame, but
//! only while the I/O task has proven itself alive recently. The I/O task ticks
//! this flag every loop iteration (~1 ms cadence, USB connected or not), so
//! silence really means a wedged task — the feeder then stops feeding and lets
//! the RWDT reset the device. See `fw-esp32c6/src/recovery/watchdog.rs` for the
//! consumer shape P3 rewrites for the abort tier.

use core::sync::atomic::{AtomicBool, Ordering};

pub(crate) static IO_ALIVE: AtomicBool = AtomicBool::new(false);

/// Called by the I/O task every loop iteration.
pub fn note_io_alive() {
    IO_ALIVE.store(true, Ordering::Release);
}
