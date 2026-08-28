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

use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

use lpc_wire::server::LinkCounters;

static PARSE_FAILURES: AtomicU32 = AtomicU32::new(0);
static RX_ERRORS: AtomicU32 = AtomicU32::new(0);
static QUEUE_FULL_DROPS: AtomicU32 = AtomicU32::new(0);
static STALE_PARTIAL_FLUSHES: AtomicU32 = AtomicU32::new(0);

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

/// Snapshot for the heartbeat. Always `Some` on these targets — a serial
/// link exists by construction; zeros mean "no loss", which is itself
/// evidence.
pub fn current() -> Option<LinkCounters> {
    Some(LinkCounters {
        parse_failures: PARSE_FAILURES.load(Relaxed),
        rx_errors: RX_ERRORS.load(Relaxed),
        queue_full_drops: QUEUE_FULL_DROPS.load(Relaxed),
        stale_partial_flushes: STALE_PARTIAL_FLUSHES.load(Relaxed),
    })
}
