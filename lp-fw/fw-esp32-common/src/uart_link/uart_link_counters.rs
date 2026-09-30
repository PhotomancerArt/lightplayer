//! What the classic's UART host link recovered from and what the board's edge
//! dropped, as the heartbeat reads it — the counterpart of
//! [`crate::usb_link::usb_link_counters`] (plan `lp-link-usb-cutover`, D7),
//! with a UART's edge in place of USB's.
//!
//! Three parts:
//!
//! - lp-link's own [`LinkCounters`] (resends, damaged frames, duplicates,
//!   stale-session frames, resets, …), published by the link task after
//!   every pass, so a reader never has to borrow the link;
//! - the [`LinkCounterTally`](lpc_wire::LinkCounterTally) beside it (resets
//!   by reason, stalls, payload errors). [`heartbeat`] folds the two into the
//!   wire's `LinkCounters`, the heartbeat's `link` field — the same shape the
//!   C6 and S3 report, no new wire field;
//! - the board edge's own counters ([`EdgeCounters`]): relaxed atomics, so
//!   the I/O task may bump them from its interrupt executor, where it may not
//!   log (ADR 2026-08-25). They are not on the wire: the link task logs each
//!   increase (thread side), and what they cost the link shows up in its own
//!   counters anyway — a byte the FIFO dropped is a damaged frame, resent.

use core::cell::RefCell;
use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

use critical_section::Mutex;
use lp_link::LinkCounters;

static LINK: Mutex<RefCell<Option<LinkCounters>>> = Mutex::new(RefCell::new(None));

#[cfg(feature = "server")]
static TALLY: Mutex<RefCell<lpc_wire::LinkCounterTally>> =
    Mutex::new(RefCell::new(lpc_wire::LinkCounterTally::new()));

static REPLIES_DROPPED_FULL: AtomicU32 = AtomicU32::new(0);
static REPLIES_DROPPED_NO_LINK: AtomicU32 = AtomicU32::new(0);
static RX_ERRORS: AtomicU32 = AtomicU32::new(0);
static RX_PIPE_OVERFLOW_BYTES: AtomicU32 = AtomicU32::new(0);
static WRITE_FAILURES: AtomicU32 = AtomicU32::new(0);

/// The board edge's counters, since boot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EdgeCounters {
    /// Replies refused because the link's send budget stayed full (a stalled
    /// host, or one that stopped reading): dropped, and the peer told.
    pub replies_dropped_full: u32,
    /// Messages sent with no host session to take them (no host, a session
    /// that reset while the reply waited, or before the owed hello).
    pub replies_dropped_no_link: u32,
    /// UART RX errors (FIFO overflow, framing, parity): bytes the hardware
    /// lost before the I/O task could take them.
    pub rx_errors: u32,
    /// Bytes the I/O task took from the FIFO and had no room for in the RX
    /// pipe (the link task fell that far behind).
    pub rx_pipe_overflow_bytes: u32,
    /// UART writes that timed out or failed (a wedged peripheral: a UART
    /// clocks bytes out whether or not anyone listens).
    pub write_failures: u32,
}

/// The heartbeat's `link` field: this link's counters with the tally's, or
/// `None` before the link task's first pass.
#[cfg(feature = "server")]
pub fn heartbeat() -> Option<lpc_wire::server::LinkCounters> {
    let link = critical_section::with(|cs| LINK.borrow_ref(cs).clone())?;
    Some(critical_section::with(|cs| {
        TALLY.borrow_ref(cs).snapshot(&link)
    }))
}

/// The link task's copy of the link's counters, after a pass.
pub fn publish(counters: &LinkCounters) {
    critical_section::with(|cs| {
        *LINK.borrow_ref_mut(cs) = Some(counters.clone());
    });
}

/// The edge counters.
pub fn edge() -> EdgeCounters {
    EdgeCounters {
        replies_dropped_full: REPLIES_DROPPED_FULL.load(Relaxed),
        replies_dropped_no_link: REPLIES_DROPPED_NO_LINK.load(Relaxed),
        rx_errors: RX_ERRORS.load(Relaxed),
        rx_pipe_overflow_bytes: RX_PIPE_OVERFLOW_BYTES.load(Relaxed),
        write_failures: WRITE_FAILURES.load(Relaxed),
    }
}

/// **I/O task.** The UART reported an RX error.
pub fn note_rx_error() {
    RX_ERRORS.fetch_add(1, Relaxed);
}

/// **I/O task.** A UART write timed out or failed.
pub fn note_write_failure() {
    WRITE_FAILURES.fetch_add(1, Relaxed);
}

/// **I/O task.** `bytes` did not fit the RX pipe.
pub(crate) fn note_rx_pipe_overflow(bytes: usize) {
    RX_PIPE_OVERFLOW_BYTES.fetch_add(bytes.min(u32::MAX as usize) as u32, Relaxed);
}

/// Whether the link is stalled now; the link task calls this every pass.
pub(crate) fn note_stalled(stalled: bool) {
    #[cfg(feature = "server")]
    critical_section::with(|cs| {
        TALLY.borrow_ref_mut(cs).note_stalled(stalled);
    });
    #[cfg(not(feature = "server"))]
    let _ = stalled;
}

/// A link reset, and why.
#[cfg(feature = "server")]
pub(crate) fn note_reset(reason: lp_link::ResetReason) {
    critical_section::with(|cs| TALLY.borrow_ref_mut(cs).note_reset(reason));
}

/// A proto-channel message from the host arrived intact and did not decode.
#[cfg(feature = "server")]
pub(crate) fn note_payload_error() {
    critical_section::with(|cs| TALLY.borrow_ref_mut(cs).note_payload_error());
}

#[cfg_attr(
    not(feature = "server"),
    allow(dead_code, reason = "the server transport's")
)]
pub(crate) fn note_reply_dropped_full() {
    REPLIES_DROPPED_FULL.fetch_add(1, Relaxed);
}

#[cfg_attr(
    not(feature = "server"),
    allow(dead_code, reason = "the server transport's")
)]
pub(crate) fn note_reply_dropped_no_link() {
    REPLIES_DROPPED_NO_LINK.fetch_add(1, Relaxed);
}
