//! Serial-link loss counters — the device-side end of "loss is never
//! silent" (2026-08-26 inbound-loss defect: every drop used to vanish with
//! zero evidence).
//!
//! Each drop site bumps one relaxed atomic; the heartbeat attaches a
//! [`LinkCounters`] snapshot every interval. Bumps are bare atomics only, so
//! they are safe from any context — including the classic's io_task, which
//! polls on an interrupt executor where logging and allocation are banned
//! (ADR 2026-08-25 hard rules). Reporting happens thread-side in the server
//! loop.
//!
//! Since M6 P1b the same snapshot also carries the USB link's *connection*
//! history — when it last latched "the host is not draining me" and when it
//! last recovered — because that is a loss too, and the loudest one: while it
//! holds, every protocol write and every log line is dropped on the floor,
//! including the log line that says so. The device's own clock is the only
//! witness to a silence by definition nobody could hear.

use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

#[cfg(feature = "server")]
use lpc_wire::server::LinkCounters;

static PARSE_FAILURES: AtomicU32 = AtomicU32::new(0);
static RX_ERRORS: AtomicU32 = AtomicU32::new(0);
static QUEUE_FULL_DROPS: AtomicU32 = AtomicU32::new(0);
static STALE_PARTIAL_FLUSHES: AtomicU32 = AtomicU32::new(0);

/// Milliseconds since boot at the most recent "host is not draining" latch,
/// or [`NEVER`] if the link has never latched this boot.
///
/// `pub` and named rather than a private cell because it is also read from
/// **outside** the running firmware: the emulator's `--probe
/// fw_esp32_common::serial::link_counters::HOST_NOT_DRAINING_MS@<ms>` resolves
/// this demangled path and reads the word directly, which is how a gate can
/// ask about the latch on a configuration that has no heartbeat reader
/// attached at all.
pub static HOST_NOT_DRAINING_MS: AtomicU32 = AtomicU32::new(NEVER);
/// Milliseconds since boot at the most recent recovery, or [`NEVER`].
/// Probeable under the same rule as [`HOST_NOT_DRAINING_MS`].
pub static HOST_DRAINING_AGAIN_MS: AtomicU32 = AtomicU32::new(NEVER);
/// How many times the link has latched "not draining" since boot.
/// Probeable under the same rule as [`HOST_NOT_DRAINING_MS`].
pub static NOT_DRAINING_COUNT: AtomicU32 = AtomicU32::new(0);

/// The in-memory spelling of "this has not happened yet".
///
/// `u32::MAX` rather than `0`, because 0 ms since boot is a real instant and a
/// latch there is exactly what a host-absent boot would produce. The wire
/// spelling is `None` — [`current`] does the translation once, here, so no
/// reader ever has to know the sentinel.
pub const NEVER: u32 = u32::MAX;

/// An `M!` line's JSON failed to parse (torn or spliced frame).
pub fn bump_parse_failure() {
    PARSE_FAILURES.fetch_add(1, Relaxed);
}

/// A hardware RX error (overflow/parity/framing) dropped a partial line.
pub fn bump_rx_error() {
    RX_ERRORS.fetch_add(1, Relaxed);
}

/// A parsed `M!` line was dropped because the inbound queue was full.
pub fn bump_queue_full_drop() {
    QUEUE_FULL_DROPS.fetch_add(1, Relaxed);
}

/// A stale partial line was discarded at a session boundary.
pub fn bump_stale_partial_flush() {
    STALE_PARTIAL_FLUSHES.fetch_add(1, Relaxed);
}

/// The link latched "the host is not draining me" at `now_ms` since boot.
///
/// Called from the chip's USB connection monitor. The latch also emits a log
/// line, which on a USB link is dropped by the very state it reports (the
/// outgoing queue is gated on `is_connected()`), so this stamp is the only
/// record of the moment that survives to be read later.
pub fn note_host_not_draining(now_ms: u32) {
    HOST_NOT_DRAINING_MS.store(now_ms, Relaxed);
    NOT_DRAINING_COUNT.fetch_add(1, Relaxed);
}

/// The link decided the host is draining it again at `now_ms` since boot.
pub fn note_host_draining_again(now_ms: u32) {
    HOST_DRAINING_AGAIN_MS.store(now_ms, Relaxed);
}

/// When the link last latched "not draining", as the wire spells it:
/// `None` = never this boot.
pub fn host_not_draining_ms() -> Option<u32> {
    stamp(&HOST_NOT_DRAINING_MS)
}

/// When the link last recovered, as the wire spells it.
pub fn host_draining_again_ms() -> Option<u32> {
    stamp(&HOST_DRAINING_AGAIN_MS)
}

/// How many silences there have been since boot.
pub fn not_draining_count() -> u32 {
    NOT_DRAINING_COUNT.load(Relaxed)
}

/// Snapshot for the heartbeat. Always `Some` on these targets — a serial
/// link exists by construction; zeros mean "no loss", which is itself
/// evidence.
#[cfg(feature = "server")]
pub fn current() -> Option<LinkCounters> {
    Some(LinkCounters {
        parse_failures: PARSE_FAILURES.load(Relaxed),
        rx_errors: RX_ERRORS.load(Relaxed),
        queue_full_drops: QUEUE_FULL_DROPS.load(Relaxed),
        stale_partial_flushes: STALE_PARTIAL_FLUSHES.load(Relaxed),
        host_not_draining_ms: host_not_draining_ms(),
        host_draining_again_ms: host_draining_again_ms(),
        not_draining_count: not_draining_count(),
    })
}

/// [`NEVER`] becomes `None` on the wire; anything else is a real instant.
fn stamp(cell: &AtomicU32) -> Option<u32> {
    match cell.load(Relaxed) {
        NEVER => None,
        ms => Some(ms),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sentinel translation, which is the only logic in this module.
    /// The statics themselves are process-global and the firmware has one
    /// link, so they are not exercised here — the monitor's transitions are
    /// (`crate::serial::usb_connection`).
    #[test]
    fn never_is_absent_on_the_wire_and_zero_is_not() {
        let cell = AtomicU32::new(NEVER);
        assert_eq!(stamp(&cell), None);
        cell.store(0, Relaxed);
        assert_eq!(
            stamp(&cell),
            Some(0),
            "0 ms since boot is a real instant, not `never`"
        );
    }
}
