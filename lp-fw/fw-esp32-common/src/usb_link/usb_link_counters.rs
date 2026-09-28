//! What the USB host link recovered from and what the board's edge dropped,
//! as the heartbeat reads it (D7 of plan `lp-link-usb-cutover`).
//!
//! Three parts:
//!
//! - lp-link's own [`LinkCounters`] (resends, damaged frames, duplicates,
//!   stale-session frames, resets, …), published by the link task after
//!   every pass, so a reader never has to borrow the link;
//! - the [`LinkCounterTally`](lpc_wire::LinkCounterTally) beside it (resets
//!   by reason, stalls, payload errors): lp-link hands those up as events and
//!   states, not counts. [`heartbeat`] folds the two into the wire's
//!   `LinkCounters`, the heartbeat's `link` field;
//! - the board edge's own counters ([`EdgeCounters`]), relaxed atomics bumped
//!   where the event happens: replies the transport could not queue, frame
//!   writes the host did not drain. Not on the wire; logged where they
//!   happen and readable here.

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
static WRITE_TIMEOUTS: AtomicU32 = AtomicU32::new(0);
static WRITE_TIMEOUTS_LIVE: AtomicU32 = AtomicU32::new(0);
static LOST_WAKES: AtomicU32 = AtomicU32::new(0);
static WRITE_ERRORS: AtomicU32 = AtomicU32::new(0);
static FRAMES_DISCARDED_NO_HOST: AtomicU32 = AtomicU32::new(0);

/// The board edge's counters, since boot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EdgeCounters {
    /// Replies refused because the link's send budget stayed full (a stalled
    /// host, or one that stopped reading): dropped, and the peer told.
    pub replies_dropped_full: u32,
    /// Messages sent with no host session to take them (no host, a session
    /// that reset while the reply waited, or before the owed hello).
    pub replies_dropped_no_link: u32,
    /// Frame writes the host did not drain in time.
    pub write_timeouts: u32,
    /// Of those, the ones while the link was up and hearing its peer: a
    /// host WAS draining, so the write should not have timed out.
    pub write_timeouts_live: u32,
    /// Of the live ones, those that ended with the IN endpoint free: the
    /// host took the packet and the write never woke (the esp-hal ISR
    /// defect, docs/defects/2026-09-26-esp-hals-usb-isr-…).
    pub lost_wakes: u32,
    /// Frame writes that failed outright.
    pub write_errors: u32,
    /// Frames the link produced while no USB host enumerated the board
    /// (no SOF): not written, left for the link's resend and give-up.
    pub frames_discarded_no_host: u32,
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
        write_timeouts: WRITE_TIMEOUTS.load(Relaxed),
        write_timeouts_live: WRITE_TIMEOUTS_LIVE.load(Relaxed),
        lost_wakes: LOST_WAKES.load(Relaxed),
        write_errors: WRITE_ERRORS.load(Relaxed),
        frames_discarded_no_host: FRAMES_DISCARDED_NO_HOST.load(Relaxed),
    }
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

/// A frame write timed out; `live`: the link was up and hearing its peer;
/// `in_ep_free`: the endpoint's send buffer was free when it did.
pub(crate) fn note_write_timeout(live: bool, in_ep_free: bool) {
    WRITE_TIMEOUTS.fetch_add(1, Relaxed);
    if live {
        WRITE_TIMEOUTS_LIVE.fetch_add(1, Relaxed);
        if in_ep_free {
            LOST_WAKES.fetch_add(1, Relaxed);
        }
    }
}

pub(crate) fn note_write_error() {
    WRITE_ERRORS.fetch_add(1, Relaxed);
}

pub(crate) fn note_frame_discarded_no_host() {
    FRAMES_DISCARDED_NO_HOST.fetch_add(1, Relaxed);
}
