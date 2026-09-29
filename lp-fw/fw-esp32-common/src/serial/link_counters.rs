//! `M!`-link loss counters — the device-side end of "loss is never silent"
//! (2026-08-26 inbound-loss defect: every drop used to vanish with zero
//! evidence) on the transport that still speaks `M!` lines: the classic's UART
//! (plan `lp-link-usb-cutover`, D3; the BLE links moved to lp-link in plan
//! `ble-on-lp-link`, and count per link in the mux).
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

/// Snapshot for the heartbeat of an image whose host link speaks `M!` (the
/// classic). Always `Some` on these targets — a serial link exists by
/// construction; zeros mean "no loss", which is itself evidence.
///
/// Since `WIRE_PROTO_VERSION` 30 the heartbeat's `link` object is lp-link's
/// counters (D7), and an `M!` link reports its loss under the nearest names:
/// a torn line or an RX error is a damaged frame, a full inbound queue is
/// `rxNoRoom`, a stale partial line is a stale partial. A USB-link image
/// (C6, S3) reports its link's own counters instead
/// (`crate::usb_link::usb_link_counters::heartbeat`).
#[cfg(feature = "server")]
pub fn current() -> Option<LinkCounters> {
    Some(LinkCounters {
        damaged: PARSE_FAILURES
            .load(Relaxed)
            .saturating_add(RX_ERRORS.load(Relaxed)),
        rx_no_room: QUEUE_FULL_DROPS.load(Relaxed),
        stale_partials: STALE_PARTIAL_FLUSHES.load(Relaxed),
        ..LinkCounters::default()
    })
}
