//! The medium: a frame one participant hands over, offered to every other
//! participant a stated number of guest cycles later.
//!
//! # Why this is here and not in the chip crate
//!
//! This crate holds no chip numbers (see the crate docs), and a medium that
//! carries bytes between machines needs none: not a register offset, not an
//! interrupt source, not a frame format. It does not know that the frame it
//! carries is an 802.11 vendor-specific action frame, that the sender armed
//! it by writing a descriptor pointer into a radio register, or that the
//! receiver will be told about it with an interrupt. It knows a byte string,
//! who handed it over, and at which guest cycle — which is the whole of what
//! "two machines can hear each other" means. The chip crate reads the bytes
//! out of one machine's RAM and writes them into another's; this module is
//! only the queue in between.
//!
//! It is the sibling of [`crate::pins`] for frames: one shared state on the
//! outside of the machines, with the chip layer driving both ends, so neither
//! machine ever sees the other.
//!
//! # What is modelled, and what is not
//!
//! Modelled: **byte delivery on a perfect medium.** Every frame reaches every
//! other participant, in the order it was sent, exactly once, after one
//! stated latency. Nothing is lost, nothing is corrupted, nothing collides.
//!
//! Not modelled, and deliberately not: the PHY, the channel, collisions,
//! retries, RSSI, timing windows, encryption, and any notion of range. A
//! participant that is "out of range" of another does not exist here; there
//! is one medium and everybody on it hears everybody else.
//!
//! The **latency is one stated constant in guest cycles**, chosen by the
//! caller ([`PerfectAir::new`]) and never derived from a frame's own length:
//! deriving it per frame would be a PHY model, and no PHY is modelled. It
//! must not be zero — a receiver that could see a frame in the same cycle it
//! was armed would let the pair's two machines communicate inside one
//! quantum, which is not a thing radios do and not a thing a lockstep runner
//! can reproduce. Wall time never enters (plan PD5): everything here is in
//! guest cycles, so two runs of the same participants sending the same bytes
//! at the same cycles produce byte-identical deliveries.
//!
//! # Determinism
//!
//! The delivery path has no map iteration, no clock and no thread. Each
//! participant's queue is a `VecDeque` filled in send order, so
//! [`Air::take_due`] returns frames in the order they were handed over and in
//! the same order on every run.

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use lp_emu_core::sched::Cycles;

/// Which participant on the medium: an index assigned by whoever built the
/// [`PerfectAir`], stable for the life of the run.
///
/// Not an address. The medium never looks inside a frame, so it has no idea
/// what the sender calls itself on the wire; this is only "the first machine"
/// and "the second machine".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParticipantId(pub usize);

impl ParticipantId {
    pub const fn index(self) -> usize {
        self.0
    }
}

impl core::fmt::Display for ParticipantId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// A frame on the medium: the bytes, who sent them, and when.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AirFrame {
    /// Who handed it over. Never the participant it is delivered to.
    pub from: ParticipantId,
    /// The guest cycle of the handover, on the *sender's* clock. In a
    /// lockstep run the two clocks are the same clock, which is the reason
    /// lockstep exists.
    pub at: Cycles,
    /// When the medium is willing to offer it: `at + latency`.
    pub due: Cycles,
    /// The frame, verbatim.
    pub bytes: Vec<u8>,
}

/// A medium frames travel on.
///
/// Two calls, both driven from outside the machines: one participant hands a
/// frame over, and every other participant asks what has become deliverable.
pub trait Air {
    /// One participant hands the medium a frame at guest cycle `at`.
    ///
    /// A frame from a participant the medium does not know is dropped and
    /// counted ([`PerfectAir::dropped`] on the concrete type) rather than
    /// panicking: an emulator that aborted mid-run would tell you less than
    /// one that says so at the end.
    fn send(&mut self, from: ParticipantId, at: Cycles, bytes: &[u8]);

    /// Everything deliverable to `to` at or before `now`, in send order.
    ///
    /// Each frame is returned exactly once, to each participant but its
    /// sender. A frame whose `due` is past `now` stays queued.
    fn take_due(&mut self, to: ParticipantId, now: Cycles) -> Vec<AirFrame>;
}

/// The perfect medium: one stated latency, no losses, no PHY.
///
/// Participants are fixed at construction. `send` fans a frame out into one
/// queue per other participant, so delivery order is the send order by
/// construction and nothing in the path depends on a hash.
#[derive(Debug)]
pub struct PerfectAir {
    latency: Cycles,
    /// One queue per participant, in participant order. Never a map.
    queues: Vec<VecDeque<AirFrame>>,
    sent: u64,
    delivered: u64,
    dropped: u64,
}

impl PerfectAir {
    /// A medium with `participants` participants and a latency of `latency`
    /// guest cycles.
    ///
    /// `latency` is the caller's stated constant and this module does not
    /// choose it — the chip crate does, because "how many cycles is 672 µs"
    /// is a chip number. A latency of zero is refused by
    /// [`Self::try_new`]; this constructor panics on one, because a zero
    /// latency is a construction bug and not a run-time condition.
    pub fn new(participants: usize, latency: Cycles) -> Self {
        Self::try_new(participants, latency)
            .expect("PerfectAir: the latency must be at least one guest cycle")
    }

    /// As [`Self::new`], but `None` for a zero latency.
    pub fn try_new(participants: usize, latency: Cycles) -> Option<Self> {
        if latency == 0 {
            return None;
        }
        Some(Self {
            latency,
            queues: (0..participants).map(|_| VecDeque::new()).collect(),
            sent: 0,
            delivered: 0,
            dropped: 0,
        })
    }

    /// The stated latency, in guest cycles. A transcript sidecar states this.
    pub const fn latency(&self) -> Cycles {
        self.latency
    }

    /// How many participants are on the medium.
    pub fn participants(&self) -> usize {
        self.queues.len()
    }

    /// Frames handed over, deliveries made, and frames dropped because the
    /// sender was not a participant.
    pub const fn sent(&self) -> u64 {
        self.sent
    }

    pub const fn delivered(&self) -> u64 {
        self.delivered
    }

    pub const fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Frames queued for `to` that are not deliverable yet.
    pub fn pending(&self, to: ParticipantId) -> usize {
        self.queues.get(to.index()).map_or(0, VecDeque::len)
    }

    /// The earliest cycle at which anything is deliverable to anyone, or
    /// `None` when the medium is empty. A runner may use it to bound a
    /// quantum; the lockstep runner does not need to, because its quantum is
    /// already no larger than the latency.
    pub fn next_due(&self) -> Option<Cycles> {
        self.queues
            .iter()
            .filter_map(|q| q.front().map(|f| f.due))
            .min()
    }
}

impl Air for PerfectAir {
    fn send(&mut self, from: ParticipantId, at: Cycles, bytes: &[u8]) {
        if from.index() >= self.queues.len() {
            self.dropped += 1;
            return;
        }
        self.sent += 1;
        let due = at.saturating_add(self.latency);
        for to in 0..self.queues.len() {
            if to == from.index() {
                // A frame is never delivered back to its sender. A radio does
                // not hear itself, and a machine that did would answer its own
                // broadcast.
                continue;
            }
            self.queues[to].push_back(AirFrame {
                from,
                at,
                due,
                bytes: bytes.to_vec(),
            });
        }
    }

    fn take_due(&mut self, to: ParticipantId, now: Cycles) -> Vec<AirFrame> {
        let Some(queue) = self.queues.get_mut(to.index()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        // The queue is in send order and every frame carries the same
        // latency, so `due` is non-decreasing along it: the first frame that
        // is not due stops the walk.
        while queue.front().is_some_and(|f| f.due <= now) {
            out.push(queue.pop_front().expect("checked"));
        }
        self.delivered += out.len() as u64;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: ParticipantId = ParticipantId(0);
    const B: ParticipantId = ParticipantId(1);
    const C: ParticipantId = ParticipantId(2);

    #[test]
    fn a_frame_is_offered_only_after_the_stated_latency() {
        let mut air = PerfectAir::new(2, 100);
        air.send(A, 1_000, b"hello");
        assert!(air.take_due(B, 1_099).is_empty(), "not due one cycle early");
        assert_eq!(air.pending(B), 1);
        let got = air.take_due(B, 1_100);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].bytes, b"hello");
        assert_eq!(got[0].from, A);
        assert_eq!(got[0].at, 1_000);
        assert_eq!(got[0].due, 1_100);
        assert!(air.take_due(B, 9_999).is_empty(), "delivered exactly once");
    }

    #[test]
    fn a_frame_is_never_delivered_back_to_its_sender() {
        let mut air = PerfectAir::new(2, 10);
        air.send(A, 0, b"mine");
        assert!(air.take_due(A, 1_000_000).is_empty());
        assert_eq!(air.take_due(B, 1_000_000).len(), 1);
    }

    #[test]
    fn send_order_is_delivery_order() {
        let mut air = PerfectAir::new(2, 10);
        for n in 0..8u8 {
            air.send(A, u64::from(n) * 3, &[n]);
        }
        let got = air.take_due(B, 1_000);
        let seen: Vec<u8> = got.iter().map(|f| f.bytes[0]).collect();
        assert_eq!(seen, (0..8u8).collect::<Vec<_>>());
        // And a partial take keeps the tail in order.
        let mut air = PerfectAir::new(2, 10);
        for n in 0..8u8 {
            air.send(A, u64::from(n) * 3, &[n]);
        }
        let first = air.take_due(B, 19); // due = at + 10, so n = 0..3
        assert_eq!(
            first.iter().map(|f| f.bytes[0]).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        let rest = air.take_due(B, 1_000);
        assert_eq!(
            rest.iter().map(|f| f.bytes[0]).collect::<Vec<_>>(),
            vec![4, 5, 6, 7]
        );
    }

    #[test]
    fn three_participants_each_hear_the_other_two() {
        let mut air = PerfectAir::new(3, 5);
        air.send(A, 0, b"a");
        air.send(B, 1, b"b");
        air.send(C, 2, b"c");
        let heard = |air: &mut PerfectAir, who| {
            air.take_due(who, 1_000)
                .into_iter()
                .map(|f| (f.from, f.bytes))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            heard(&mut air, A),
            vec![(B, b"b".to_vec()), (C, b"c".to_vec())]
        );
        assert_eq!(
            heard(&mut air, B),
            vec![(A, b"a".to_vec()), (C, b"c".to_vec())]
        );
        assert_eq!(
            heard(&mut air, C),
            vec![(A, b"a".to_vec()), (B, b"b".to_vec())]
        );
        assert_eq!(air.sent(), 3);
        assert_eq!(air.delivered(), 6);
    }

    #[test]
    fn the_same_traffic_delivers_identically_twice() {
        let run = || {
            let mut air = PerfectAir::new(3, 64);
            let mut log = Vec::new();
            for step in 0..16u64 {
                if step % 3 == 0 {
                    air.send(A, step * 32, &[b'a', step as u8]);
                }
                if step % 5 == 0 {
                    air.send(C, step * 32, &[b'c', step as u8]);
                }
                for who in [A, B, C] {
                    for f in air.take_due(who, step * 32) {
                        log.push((who.index(), f.from.index(), f.at, f.due, f.bytes));
                    }
                }
            }
            log
        };
        assert_eq!(run(), run());
        assert!(!run().is_empty());
    }

    #[test]
    fn a_zero_latency_is_refused_and_an_unknown_sender_is_counted() {
        assert!(PerfectAir::try_new(2, 0).is_none());
        let mut air = PerfectAir::new(2, 1);
        air.send(ParticipantId(7), 0, b"nobody");
        assert_eq!(air.dropped(), 1);
        assert_eq!(air.sent(), 0);
        assert!(air.take_due(B, 1_000).is_empty());
        assert!(air.take_due(ParticipantId(7), 1_000).is_empty());
    }

    #[test]
    fn next_due_is_the_earliest_queued_delivery() {
        let mut air = PerfectAir::new(2, 100);
        assert_eq!(air.next_due(), None);
        air.send(A, 40, b"x");
        air.send(B, 10, b"y");
        assert_eq!(air.next_due(), Some(110), "B's frame was handed over first");
        assert_eq!(air.latency(), 100);
        assert_eq!(air.participants(), 2);
    }
}
