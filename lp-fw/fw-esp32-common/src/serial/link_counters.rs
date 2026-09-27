//! `M!`-link loss counters — the device-side end of "loss is never silent"
//! (2026-08-26 inbound-loss defect: every drop used to vanish with zero
//! evidence) on the transports that still speak `M!` lines: the BLE links and
//! the classic's UART (plan `lp-link-usb-cutover`, D3).
//!
//! Each drop site bumps one relaxed atomic; the heartbeat attaches a
//! `LinkCounters` snapshot every interval. Bumps are bare atomics only, so
//! they are safe from any context — including the classic's io_task, which
//! polls on an interrupt executor where logging and allocation are banned
//! (ADR 2026-08-25 hard rules). Reporting happens thread-side in the server
//! loop.
//!
//! The C6/S3 USB link runs lp-link now, which counts its own recoveries
//! (`crate::usb_link::usb_link_counters`, feature `usb-link`). Its old
//! "host not draining" latch and the stamps that recorded it are gone with
//! it (D8).

use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

#[cfg(feature = "server")]
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
///
/// ⚠️ **Integration seam (P1, D7).** The wire type is being rewritten to
/// lp-link's counters in phase P1. On a USB-link image (feature `usb-link`)
/// the integration step fills it from
/// `crate::usb_link::usb_link_counters::snapshot()`; until then the three
/// not-draining fields, whose latch is gone, are reported as never/zero.
#[cfg(feature = "server")]
pub fn current() -> Option<LinkCounters> {
    Some(LinkCounters {
        parse_failures: PARSE_FAILURES.load(Relaxed),
        rx_errors: RX_ERRORS.load(Relaxed),
        queue_full_drops: QUEUE_FULL_DROPS.load(Relaxed),
        stale_partial_flushes: STALE_PARTIAL_FLUSHES.load(Relaxed),
        // The not-draining latch these reported is gone (D8); P1 replaces
        // the fields with lp-link's counters.
        host_not_draining_ms: None,
        host_draining_again_ms: None,
        not_draining_count: 0,
    })
}
