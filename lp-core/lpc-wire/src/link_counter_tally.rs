//! What an edge counts about its link beyond what lp-link counts itself.
//!
//! lp-link hands resets up as events and a stall as a state
//! ([`Link::is_stalled`](lp_link::Link::is_stalled)); the edge driving the
//! link (the board's link task, a host's [`WireLinkPort`](crate::WireLinkPort))
//! keeps one [`LinkCounterTally`] beside it and asks it for the heartbeat's
//! [`LinkCounters`]. Sans-IO and allocation-free, so the board can keep one
//! in a `static`.

use lp_link::{LinkEvent, ResetReason};

use crate::server::{LinkCounters, LinkResets};

/// Resets by reason, stalls and payload errors for one link. See the module
/// docs.
#[derive(Debug, Clone, Default)]
pub struct LinkCounterTally {
    resets: LinkResets,
    stalls: u32,
    stalled: bool,
    payload_errors: u32,
}

impl LinkCounterTally {
    pub const fn new() -> Self {
        LinkCounterTally {
            resets: LinkResets {
                total: 0,
                peer_restarted: 0,
                retry_limit: 0,
                protocol_error: 0,
                requested: 0,
            },
            stalls: 0,
            stalled: false,
            payload_errors: 0,
        }
    }

    /// Look at every event the link hands up (only resets count here).
    pub fn note_event(&mut self, event: &LinkEvent) {
        if let LinkEvent::Reset { reason, .. } = event {
            self.note_reset(*reason);
        }
    }

    /// A reset, and why.
    pub fn note_reset(&mut self, reason: ResetReason) {
        let slot = match reason {
            ResetReason::PeerRestarted => &mut self.resets.peer_restarted,
            ResetReason::RetryLimit => &mut self.resets.retry_limit,
            ResetReason::ProtocolError => &mut self.resets.protocol_error,
            ResetReason::Requested => &mut self.resets.requested,
        };
        *slot = slot.saturating_add(1);
        // A stall ends with the session.
        self.stalled = false;
    }

    /// Whether the link is stalled now (call it as often as convenient).
    /// Returns `true` when this call saw a stall begin.
    pub fn note_stalled(&mut self, stalled: bool) -> bool {
        let began = stalled && !self.stalled;
        if began {
            self.stalls = self.stalls.saturating_add(1);
        }
        self.stalled = stalled;
        began
    }

    /// Whether the last [`note_stalled`](Self::note_stalled) saw a stall.
    pub fn is_stalled(&self) -> bool {
        self.stalled
    }

    /// A proto-channel message arrived intact and did not decode.
    pub fn note_payload_error(&mut self) {
        self.payload_errors = self.payload_errors.saturating_add(1);
    }

    /// The link's counters, with this tally's.
    pub fn snapshot(&self, link: &lp_link::LinkCounters) -> LinkCounters {
        let mut counters = LinkCounters::from(link);
        counters.resets = LinkResets {
            total: link.resets,
            ..self.resets
        };
        counters.stalls = self.stalls;
        counters.payload_errors = self.payload_errors;
        counters
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resets_are_counted_by_reason_and_the_total_is_the_links() {
        let mut tally = LinkCounterTally::new();
        for reason in [
            ResetReason::PeerRestarted,
            ResetReason::PeerRestarted,
            ResetReason::RetryLimit,
            ResetReason::Requested,
        ] {
            tally.note_event(&LinkEvent::Reset {
                reason,
                generation: 1,
            });
        }
        tally.note_event(&LinkEvent::Up { generation: 1 });
        let link = lp_link::LinkCounters {
            resets: 4,
            ..Default::default()
        };
        let resets = tally.snapshot(&link).resets;
        assert_eq!(
            resets,
            LinkResets {
                total: 4,
                peer_restarted: 2,
                retry_limit: 1,
                protocol_error: 0,
                requested: 1,
            }
        );
    }

    #[test]
    fn a_stall_counts_once_until_it_ends() {
        let mut tally = LinkCounterTally::new();
        assert!(!tally.note_stalled(false));
        assert!(tally.note_stalled(true));
        assert!(!tally.note_stalled(true), "still the same stall");
        tally.note_stalled(false);
        assert!(tally.note_stalled(true));
        tally.note_payload_error();
        let counters = tally.snapshot(&lp_link::LinkCounters::default());
        assert_eq!((counters.stalls, counters.payload_errors), (2, 1));
    }
}
