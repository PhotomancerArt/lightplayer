//! A sans-IO schedule for mDNS's own announcement rule (RFC 6762 §8.3):
//! two unsolicited announcements, 1 s apart, whenever an address is
//! acquired or changes, plus a one-shot goodbye on a clean leave.
//!
//! Time is caller-supplied milliseconds; nothing here reads a clock.

/// The gap between the two startup announcements.
const ANNOUNCE_GAP_MS: u64 = 1000;

/// How many times to announce after [`MdnsAnnounce::address_acquired`].
const ANNOUNCE_COUNT: u8 = 2;

/// Tracks when the next announcement (or goodbye) is due. The caller polls
/// [`Self::next_at`], and once it has actually sent that packet, reports it
/// back with [`Self::on_sent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MdnsAnnounce {
    /// When the next send is due, if anything is pending.
    next_at_ms: Option<u64>,
    /// How many sends (of the current kind) remain, including the pending
    /// one.
    remaining: u8,
    /// Whether the pending (and only the pending) send is a goodbye.
    goodbye: bool,
}

impl MdnsAnnounce {
    /// Nothing scheduled yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next_at_ms: None,
            remaining: 0,
            goodbye: false,
        }
    }

    /// An address was acquired (or changed) at `now_ms`: (re)start the
    /// two-announcement schedule, the first one due immediately and the
    /// second 1 s later. Replaces anything previously pending, including a
    /// goodbye that was never sent.
    pub fn address_acquired(&mut self, now_ms: u64) {
        self.next_at_ms = Some(now_ms);
        self.remaining = ANNOUNCE_COUNT;
        self.goodbye = false;
    }

    /// Schedule a goodbye (TTL-0 records) to be sent at `now_ms`, e.g. on a
    /// clean network leave. Replaces any pending regular announcement.
    pub fn goodbye(&mut self, now_ms: u64) {
        self.next_at_ms = Some(now_ms);
        self.remaining = 1;
        self.goodbye = true;
    }

    /// When the next send is due, if anything is pending. `now_ms` is
    /// unused by this schedule — every deadline here is already absolute —
    /// but is part of the signature so a caller never needs its own clock
    /// to ask the question.
    #[must_use]
    pub fn next_at(&self, _now_ms: u64) -> Option<u64> {
        self.next_at_ms
    }

    /// Whether the pending send (per [`Self::next_at`]) is the goodbye
    /// rather than a regular announcement.
    #[must_use]
    pub fn is_goodbye(&self) -> bool {
        self.goodbye
    }

    /// Report that the pending send went out at `now_ms`: advances to the
    /// next one (1 s later) or clears the schedule if that was the last.
    pub fn on_sent(&mut self, now_ms: u64) {
        if self.remaining > 0 {
            self.remaining -= 1;
        }
        if self.remaining == 0 {
            self.next_at_ms = None;
            self.goodbye = false;
        } else {
            self.next_at_ms = Some(now_ms + ANNOUNCE_GAP_MS);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_scheduled_at_first() {
        let sched = MdnsAnnounce::new();
        assert_eq!(sched.next_at(0), None);
    }

    #[test]
    fn two_announcements_one_second_apart_then_done() {
        let mut sched = MdnsAnnounce::new();
        sched.address_acquired(1000);
        assert_eq!(sched.next_at(1000), Some(1000));
        assert!(!sched.is_goodbye());

        sched.on_sent(1000);
        assert_eq!(sched.next_at(1000), Some(2000));

        sched.on_sent(2000);
        assert_eq!(sched.next_at(2000), None);
    }

    #[test]
    fn an_address_change_restarts_the_schedule() {
        let mut sched = MdnsAnnounce::new();
        sched.address_acquired(1000);
        sched.on_sent(1000);
        assert_eq!(sched.next_at(1500), Some(2000));

        sched.address_acquired(5000);
        assert_eq!(sched.next_at(5000), Some(5000));
        sched.on_sent(5000);
        assert_eq!(sched.next_at(5000), Some(6000));
    }

    #[test]
    fn a_goodbye_is_sent_once_and_replaces_a_pending_announcement() {
        let mut sched = MdnsAnnounce::new();
        sched.address_acquired(1000);

        sched.goodbye(1200);
        assert_eq!(sched.next_at(1200), Some(1200));
        assert!(sched.is_goodbye());

        sched.on_sent(1200);
        assert_eq!(sched.next_at(1200), None);
        assert!(!sched.is_goodbye());
    }
}
