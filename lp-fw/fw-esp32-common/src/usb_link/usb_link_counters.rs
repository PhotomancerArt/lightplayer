//! What the USB host link recovered from and what the board's edge dropped,
//! as the heartbeat reads it (D7 of plan `lp-link-usb-cutover`).
//!
//! Two halves:
//!
//! - lp-link's own [`LinkCounters`] (resends, damaged frames, duplicates,
//!   stale-session frames, resets, …), published by the link task after
//!   every pass, so a reader never has to borrow the link;
//! - the board edge's counters, as relaxed atomics bumped where the event
//!   happens: replies the transport could not queue, frame writes the host
//!   did not drain, resets by reason, stalls.
//!
//! ⚠️ **Integration seam (P1).** The heartbeat's wire `link` field
//! (`lpc_wire::server::LinkCounters`) is being rewritten to lp-link's
//! counters in phase P1. Until that lands, `serial::link_counters::current()`
//! still fills the old wire type and does not read this module; the
//! integration step builds the new wire value from [`snapshot`].

use core::cell::RefCell;
use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

use critical_section::Mutex;
use lp_link::{LinkCounters, ResetReason};

static LINK: Mutex<RefCell<Option<LinkCounters>>> = Mutex::new(RefCell::new(None));

static REPLIES_DROPPED_FULL: AtomicU32 = AtomicU32::new(0);
static REPLIES_DROPPED_NO_LINK: AtomicU32 = AtomicU32::new(0);
static BAD_REQUESTS: AtomicU32 = AtomicU32::new(0);
static WRITE_TIMEOUTS: AtomicU32 = AtomicU32::new(0);
static WRITE_TIMEOUTS_LIVE: AtomicU32 = AtomicU32::new(0);
static LOST_WAKES: AtomicU32 = AtomicU32::new(0);
static WRITE_ERRORS: AtomicU32 = AtomicU32::new(0);
static FRAMES_DISCARDED_NO_HOST: AtomicU32 = AtomicU32::new(0);
static STALLS: AtomicU32 = AtomicU32::new(0);
static RESETS_PEER_RESTARTED: AtomicU32 = AtomicU32::new(0);
static RESETS_RETRY_LIMIT: AtomicU32 = AtomicU32::new(0);
static RESETS_PROTOCOL_ERROR: AtomicU32 = AtomicU32::new(0);
static RESETS_REQUESTED: AtomicU32 = AtomicU32::new(0);

/// The board edge's counters, since boot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EdgeCounters {
    /// Replies refused because the link's send budget stayed full (a stalled
    /// host, or one that stopped reading): dropped, and the peer told.
    pub replies_dropped_full: u32,
    /// Messages sent with no host session to take them (no host, a session
    /// that reset while the reply waited, or before the owed hello).
    pub replies_dropped_no_link: u32,
    /// Proto-channel messages from the host that were not a client message.
    pub bad_requests: u32,
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
    /// Times the link went from hearing its peer to stalled.
    pub stalls: u32,
    /// Link resets, by reason (the total is lp-link's `resets`).
    pub resets_peer_restarted: u32,
    pub resets_retry_limit: u32,
    pub resets_protocol_error: u32,
    pub resets_requested: u32,
}

/// Both halves, as one reading.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsbLinkSnapshot {
    pub link: LinkCounters,
    pub edge: EdgeCounters,
    /// Records the log ring dropped while no host drained it.
    pub log_records_dropped: u32,
}

/// The link task's copy of the link's counters, after a pass.
pub fn publish(counters: &LinkCounters) {
    critical_section::with(|cs| {
        *LINK.borrow_ref_mut(cs) = Some(counters.clone());
    });
}

/// Everything, or `None` before the link task's first pass.
pub fn snapshot() -> Option<UsbLinkSnapshot> {
    let link = critical_section::with(|cs| LINK.borrow_ref(cs).clone())?;
    Some(UsbLinkSnapshot {
        link,
        edge: edge(),
        log_records_dropped: crate::log_ring_logger::dropped_total(),
    })
}

/// The edge counters alone.
pub fn edge() -> EdgeCounters {
    EdgeCounters {
        replies_dropped_full: REPLIES_DROPPED_FULL.load(Relaxed),
        replies_dropped_no_link: REPLIES_DROPPED_NO_LINK.load(Relaxed),
        bad_requests: BAD_REQUESTS.load(Relaxed),
        write_timeouts: WRITE_TIMEOUTS.load(Relaxed),
        write_timeouts_live: WRITE_TIMEOUTS_LIVE.load(Relaxed),
        lost_wakes: LOST_WAKES.load(Relaxed),
        write_errors: WRITE_ERRORS.load(Relaxed),
        frames_discarded_no_host: FRAMES_DISCARDED_NO_HOST.load(Relaxed),
        stalls: STALLS.load(Relaxed),
        resets_peer_restarted: RESETS_PEER_RESTARTED.load(Relaxed),
        resets_retry_limit: RESETS_RETRY_LIMIT.load(Relaxed),
        resets_protocol_error: RESETS_PROTOCOL_ERROR.load(Relaxed),
        resets_requested: RESETS_REQUESTED.load(Relaxed),
    }
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

#[cfg_attr(
    not(feature = "server"),
    allow(dead_code, reason = "the server transport's")
)]
pub(crate) fn note_bad_request() {
    BAD_REQUESTS.fetch_add(1, Relaxed);
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

pub(crate) fn note_stall() {
    STALLS.fetch_add(1, Relaxed);
}

#[cfg_attr(
    not(feature = "server"),
    allow(dead_code, reason = "the server transport's")
)]
pub(crate) fn note_reset(reason: ResetReason) {
    let cell = match reason {
        ResetReason::PeerRestarted => &RESETS_PEER_RESTARTED,
        ResetReason::RetryLimit => &RESETS_RETRY_LIMIT,
        ResetReason::ProtocolError => &RESETS_PROTOCOL_ERROR,
        ResetReason::Requested => &RESETS_REQUESTED,
    };
    cell.fetch_add(1, Relaxed);
}
