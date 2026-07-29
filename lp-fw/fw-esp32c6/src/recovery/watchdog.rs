//! RWDT arming and feed policy.
//!
//! The RTC watchdog is the layer-2 backstop for hangs that in-process
//! recovery cannot catch (an infinite loop in JIT'd shader code hangs the
//! whole device). Stage 0 resets the system after [`WATCHDOG_TIMEOUT_MS`];
//! after the reboot, `lp-recovery` blames whatever the frame stack shows.
//!
//! Feed policy (aggregator): the server loop feeds every frame, but only
//! while the I/O task has proven itself alive within
//! [`IO_SILENCE_LIMIT_MS`]. The I/O task ticks its flag every loop
//! iteration (~1 ms cadence, USB connected or not), so silence really
//! means a wedged task — we stop feeding and let the RWDT do its job.
//!
//! The timeout is deliberately generous: shader compiles legitimately take
//! seconds and run inside the server loop. We are catching escapes, not
//! enforcing latency.

use core::sync::atomic::{AtomicBool, Ordering};

use esp_hal::rtc_cntl::{Rwdt, RwdtStage};
use esp_hal::time::Duration;

/// Stage-0 reset timeout. Must exceed the worst legitimate stall of the
/// server loop (large shader compile), with margin.
pub const WATCHDOG_TIMEOUT_MS: u64 = 8_000;

/// Timeout while booting (before the server loop feeds): boot-time project
/// load + shader compiles can legitimately take much longer than a frame.
pub const BOOT_TIMEOUT_MS: u64 = 30_000;

/// How long the I/O task may go silent before the server loop stops
/// feeding on its behalf.
pub const IO_SILENCE_LIMIT_MS: u64 = 2_000;

static IO_ALIVE: AtomicBool = AtomicBool::new(false);

/// Called by the I/O task every loop iteration.
pub fn note_io_alive() {
    IO_ALIVE.store(true, Ordering::Release);
}

/// Owns the armed RWDT; fed from the server loop.
pub struct WatchdogFeeder {
    rwdt: Rwdt,
    last_io_confirm_ms: u64,
    starving_logged: bool,
    tightened: bool,
}

impl WatchdogFeeder {
    /// Arm stage 0 (system reset — the enable() default) with the generous
    /// boot timeout; the first `feed` from the server loop tightens it to
    /// [`WATCHDOG_TIMEOUT_MS`].
    pub fn start(mut rwdt: Rwdt, now_ms: u64) -> Self {
        rwdt.set_timeout(RwdtStage::Stage0, Duration::from_millis(BOOT_TIMEOUT_MS));
        rwdt.enable();
        esp_println::println!(
            "[RECOVERY] RWDT armed: boot {BOOT_TIMEOUT_MS} ms, runtime {WATCHDOG_TIMEOUT_MS} ms"
        );
        Self {
            rwdt,
            last_io_confirm_ms: now_ms,
            starving_logged: false,
            tightened: false,
        }
    }

    /// Feed if the I/O task is provably alive; otherwise let the RWDT bite.
    pub fn feed(&mut self, now_ms: u64) {
        if !self.tightened {
            self.tightened = true;
            self.rwdt.set_timeout(
                RwdtStage::Stage0,
                Duration::from_millis(WATCHDOG_TIMEOUT_MS),
            );
        }
        if IO_ALIVE.swap(false, Ordering::AcqRel) {
            self.last_io_confirm_ms = now_ms;
            self.starving_logged = false;
        }
        if now_ms.saturating_sub(self.last_io_confirm_ms) <= IO_SILENCE_LIMIT_MS {
            self.rwdt.feed();
        } else if !self.starving_logged {
            self.starving_logged = true;
            log::error!(
                "[RECOVERY] io task silent > {IO_SILENCE_LIMIT_MS} ms; withholding watchdog feed"
            );
        }
    }
}
