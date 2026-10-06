//! The host half of one engaged capability seam on one machine.
//!
//! A capability seam (a radio link, a network interface) has a guest half —
//! the firmware's adapter, calling seams — and a host half: what the
//! emulator holds for it. That host half is an **endpoint**: a bounded
//! inbound queue the guest takes from, and an outbound queue of what the
//! guest gave, which a [`super::SeamMedium`] carries to other endpoints.
//!
//! Endpoints are **per machine, never static**: each is addressed
//! `<board>/<seam>` ([`EndpointId`]), so several emulated boards in one
//! process each hold their own, and a host can wire them together.
//!
//! The queue bound and the take cap are the wake pacer's rules
//! ([`super::PacerConfig`]): an endpoint **refuses** (and counts) an event
//! past its bound rather than grow, and one take returns at most `take_cap`
//! bytes, whole events only.

use std::collections::VecDeque;
use std::fmt;

use lp_emu_core::sched::Cycles;

use crate::air::ParticipantId;

use super::seam_wake_pacer::PacerConfig;

/// `<board>/<seam>`: which machine, which seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EndpointId {
    pub board: ParticipantId,
    /// The seam implementation's label (`test`, later `ble`, `net`).
    pub seam: &'static str,
}

impl fmt::Display for EndpointId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.board.index(), self.seam)
    }
}

/// One event on an endpoint's queue: the bytes, and the guest cycle it was
/// produced at (on the producer's clock; one clock in a lockstep run).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndpointEvent {
    pub at: Cycles,
    pub bytes: Vec<u8>,
}

/// Why an event was not queued.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The inbound queue is at its bound.
    Full,
    /// The event is larger than one take may return, so no take could ever
    /// deliver it.
    TooLarge,
}

/// The host half of one engaged capability seam on one machine.
#[derive(Clone, Debug)]
pub struct SeamEndpoint {
    pub id: EndpointId,
    /// This endpoint's bit in the table's wake pending word.
    pub bit: u32,
    config: PacerConfig,
    inbound: VecDeque<EndpointEvent>,
    outbound: VecDeque<EndpointEvent>,
    refused: u64,
    taken_bytes: u64,
    taken_events: u64,
}

impl SeamEndpoint {
    pub fn new(id: EndpointId, bit: u32, config: PacerConfig) -> Self {
        Self {
            id,
            bit,
            config,
            inbound: VecDeque::new(),
            outbound: VecDeque::new(),
            refused: 0,
            taken_bytes: 0,
            taken_events: 0,
        }
    }

    /// Queue an event for the guest. Refused (and counted) past the bound.
    pub fn push_inbound(&mut self, event: EndpointEvent) -> Result<(), Refused> {
        if event.bytes.len() > self.config.take_cap {
            self.refused += 1;
            return Err(Refused::TooLarge);
        }
        if self.inbound.len() >= self.config.queue_bound {
            self.refused += 1;
            return Err(Refused::Full);
        }
        self.inbound.push_back(event);
        Ok(())
    }

    /// What one guest take returns: whole events, oldest first, at most
    /// `min(cap, take_cap)` bytes. Empty when nothing is queued (the guest's
    /// drain loop stops on a zero-length take).
    pub fn take(&mut self, cap: usize) -> Vec<u8> {
        let cap = cap.min(self.config.take_cap);
        let mut out = Vec::new();
        while let Some(front) = self.inbound.front() {
            if out.len() + front.bytes.len() > cap {
                break;
            }
            let event = self.inbound.pop_front().expect("front exists");
            out.extend_from_slice(&event.bytes);
            self.taken_events += 1;
        }
        self.taken_bytes += out.len() as u64;
        out
    }

    /// One whole event, the oldest, if it fits `min(cap, take_cap)`: for a
    /// seam whose guest takes one message per call (a network frame), where
    /// [`Self::take`]'s concatenation would lose the boundary. `None` when
    /// nothing is queued or the oldest does not fit (it stays queued).
    pub fn take_one(&mut self, cap: usize) -> Option<Vec<u8>> {
        let cap = cap.min(self.config.take_cap);
        if self.inbound.front()?.bytes.len() > cap {
            return None;
        }
        let event = self.inbound.pop_front()?;
        self.taken_events += 1;
        self.taken_bytes += event.bytes.len() as u64;
        Some(event.bytes)
    }

    /// Something the guest gave, for a medium to carry.
    pub fn push_outbound(&mut self, event: EndpointEvent) {
        self.outbound.push_back(event);
    }

    /// Everything the guest gave since the last drain, oldest first.
    pub fn drain_outbound(&mut self) -> Vec<EndpointEvent> {
        self.outbound.drain(..).collect()
    }

    pub fn has_inbound(&self) -> bool {
        !self.inbound.is_empty()
    }

    pub fn inbound_len(&self) -> usize {
        self.inbound.len()
    }

    /// Events refused: past the bound, or too large for any take.
    pub fn refused(&self) -> u64 {
        self.refused
    }

    pub fn taken_bytes(&self) -> u64 {
        self.taken_bytes
    }

    pub fn taken_events(&self) -> u64 {
        self.taken_events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_ids_are_distinct_per_board() {
        let a = EndpointId {
            board: ParticipantId(0),
            seam: "test",
        };
        let b = EndpointId {
            board: ParticipantId(1),
            ..a
        };
        assert_ne!(a, b);
        assert_eq!(a.to_string(), "0/test");
        assert_eq!(b.to_string(), "1/test");
    }

    #[test]
    fn the_inbound_queue_refuses_past_its_bound_and_counts() {
        let mut e = endpoint(PacerConfig {
            queue_bound: 2,
            ..PacerConfig::default()
        });
        assert!(e.push_inbound(event(1, &[1])).is_ok());
        assert!(e.push_inbound(event(2, &[2])).is_ok());
        assert_eq!(e.push_inbound(event(3, &[3])), Err(Refused::Full));
        assert_eq!(e.refused(), 1);
        assert_eq!(e.inbound_len(), 2);
    }

    #[test]
    fn a_take_returns_whole_events_under_the_cap() {
        let mut e = endpoint(PacerConfig {
            take_cap: 8,
            ..PacerConfig::default()
        });
        for i in 0..4u8 {
            e.push_inbound(event(u64::from(i), &[i; 3])).unwrap();
        }
        assert_eq!(
            e.take(64),
            [0, 0, 0, 1, 1, 1],
            "two whole events fit the cap of 8"
        );
        assert_eq!(e.take(4), [2, 2, 2]);
        assert_eq!(e.take(2), Vec::<u8>::new(), "a whole event never splits");
        assert_eq!(e.take(64), [3, 3, 3]);
        assert!(e.take(64).is_empty());
        assert_eq!(e.taken_events(), 4);
        assert_eq!(e.taken_bytes(), 12);
        assert_eq!(e.push_inbound(event(9, &[0; 9])), Err(Refused::TooLarge));
    }

    #[test]
    fn take_one_keeps_each_events_boundary() {
        let mut e = endpoint(PacerConfig {
            take_cap: 8,
            ..PacerConfig::default()
        });
        e.push_inbound(event(1, &[1, 1])).unwrap();
        e.push_inbound(event(2, &[2, 2, 2])).unwrap();
        assert_eq!(e.take_one(64), Some(vec![1, 1]), "one event, not two");
        assert_eq!(e.take_one(2), None, "too small a buffer: it stays queued");
        assert_eq!(e.inbound_len(), 1);
        assert_eq!(e.take_one(64), Some(vec![2, 2, 2]));
        assert_eq!(e.take_one(64), None);
        assert_eq!((e.taken_events(), e.taken_bytes()), (2, 5));
    }

    #[test]
    fn outbound_drains_in_order() {
        let mut e = endpoint(PacerConfig::default());
        e.push_outbound(event(5, b"a"));
        e.push_outbound(event(6, b"b"));
        let out = e.drain_outbound();
        assert_eq!(out.iter().map(|x| x.at).collect::<Vec<_>>(), [5, 6]);
        assert!(e.drain_outbound().is_empty());
    }

    fn endpoint(config: PacerConfig) -> SeamEndpoint {
        SeamEndpoint::new(
            EndpointId {
                board: ParticipantId(0),
                seam: "test",
            },
            1,
            config,
        )
    }

    fn event(at: Cycles, bytes: &[u8]) -> EndpointEvent {
        EndpointEvent {
            at,
            bytes: bytes.to_vec(),
        }
    }
}
