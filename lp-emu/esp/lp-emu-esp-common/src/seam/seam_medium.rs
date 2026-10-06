//! What joins the endpoints of several machines.
//!
//! A [`SeamMedium`] carries what one machine's guest gave on an endpoint to
//! other machines' endpoints: the medium **interface**. Real media — a
//! virtual LAN under the network seam, a virtual Bluetooth air under the
//! Bluetooth seam — arrive with their seams. This module has the interface
//! and a test medium, [`LoopbackMedium`].
//!
//! A host calls [`SeamMedium::deliver`] at quantum boundaries, with every
//! participating machine's endpoints. The **deterministic** driver is the
//! lockstep runner (`lp-emu-esp32c6`'s `Lockstep`): its machines share one
//! guest clock and stop at the same boundaries, so two runs deliver the same
//! events at the same cycles. Like the air (`crate::air`), a medium states
//! its latency in guest cycles and never chooses one.

use std::collections::VecDeque;

use lp_emu_core::sched::Cycles;

use super::seam_endpoint::{EndpointEvent, EndpointId, SeamEndpoint};

/// Carries endpoint events between machines.
pub trait SeamMedium {
    /// Move everything due at or before `now`: collect each endpoint's
    /// outbound events, and push every event whose time has come into its
    /// destination's inbound queue (a refusal there is the endpoint's to
    /// count).
    fn deliver(&mut self, now: Cycles, endpoints: &mut [&mut SeamEndpoint]);
}

/// A test medium: fixed routes `from → to`, one stated latency, no loss.
#[derive(Clone, Debug)]
pub struct LoopbackMedium {
    latency: Cycles,
    routes: Vec<(EndpointId, EndpointId)>,
    /// `(due, to, event)`, in the order they were collected.
    in_flight: VecDeque<(Cycles, EndpointId, EndpointEvent)>,
    carried: u64,
    delivered: u64,
    lost: u64,
}

impl LoopbackMedium {
    /// A medium with `latency` guest cycles between an event's production and
    /// its delivery.
    pub fn new(latency: Cycles) -> Self {
        Self {
            latency,
            routes: Vec::new(),
            in_flight: VecDeque::new(),
            carried: 0,
            delivered: 0,
            lost: 0,
        }
    }

    /// Everything `from`'s guest gives arrives at `to`.
    pub fn route(mut self, from: EndpointId, to: EndpointId) -> Self {
        self.routes.push((from, to));
        self
    }

    pub fn latency(&self) -> Cycles {
        self.latency
    }

    /// Events picked up, delivered, and refused by their destination (or
    /// addressed to an endpoint nobody passed in).
    pub fn carried(&self) -> u64 {
        self.carried
    }

    pub fn delivered(&self) -> u64 {
        self.delivered
    }

    pub fn lost(&self) -> u64 {
        self.lost
    }

    pub fn in_flight(&self) -> usize {
        self.in_flight.len()
    }
}

impl SeamMedium for LoopbackMedium {
    fn deliver(&mut self, now: Cycles, endpoints: &mut [&mut SeamEndpoint]) {
        for e in endpoints.iter_mut() {
            let from = e.id;
            for event in e.drain_outbound() {
                for (_, to) in self.routes.iter().filter(|(f, _)| *f == from) {
                    self.carried += 1;
                    self.in_flight
                        .push_back((event.at + self.latency, *to, event.clone()));
                }
            }
        }
        let mut waiting = VecDeque::new();
        while let Some((due, to, event)) = self.in_flight.pop_front() {
            if due > now {
                waiting.push_back((due, to, event));
                continue;
            }
            match endpoints.iter_mut().find(|e| e.id == to) {
                Some(dest) => {
                    let arrived = EndpointEvent {
                        at: due,
                        bytes: event.bytes,
                    };
                    if dest.push_inbound(arrived).is_ok() {
                        self.delivered += 1;
                    } else {
                        self.lost += 1;
                    }
                }
                None => self.lost += 1,
            }
        }
        self.in_flight = waiting;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::air::ParticipantId;
    use crate::seam::PacerConfig;

    #[test]
    fn delivery_waits_out_the_latency_and_keeps_the_order() {
        let (a, b) = (id(0), id(1));
        let mut ea = SeamEndpoint::new(a, 1, PacerConfig::default());
        let mut eb = SeamEndpoint::new(b, 1, PacerConfig::default());
        let mut m = LoopbackMedium::new(100).route(a, b).route(b, a);
        ea.push_outbound(event(10, b"one"));
        ea.push_outbound(event(20, b"two"));
        eb.push_outbound(event(15, b"back"));

        m.deliver(50, &mut [&mut ea, &mut eb]);
        assert!(!eb.has_inbound(), "not due before 110");
        assert_eq!(m.in_flight(), 3);
        m.deliver(110, &mut [&mut ea, &mut eb]);
        assert_eq!(eb.take(64), b"one");
        assert!(!ea.has_inbound(), "the reply is due at 115");
        m.deliver(120, &mut [&mut ea, &mut eb]);
        assert_eq!(eb.take(64), b"two");
        assert_eq!(ea.take(64), b"back");
        assert_eq!((m.carried(), m.delivered(), m.lost()), (3, 3, 0));
    }

    #[test]
    fn an_unrouted_endpoint_carries_nothing_and_a_full_one_loses() {
        let (a, b) = (id(0), id(1));
        let mut ea = SeamEndpoint::new(a, 1, PacerConfig::default());
        let mut eb = SeamEndpoint::new(
            b,
            1,
            PacerConfig {
                queue_bound: 1,
                ..PacerConfig::default()
            },
        );
        let mut m = LoopbackMedium::new(1).route(a, b);
        eb.push_outbound(event(0, b"nowhere"));
        ea.push_outbound(event(0, b"x"));
        ea.push_outbound(event(0, b"y"));
        m.deliver(10, &mut [&mut ea, &mut eb]);
        assert_eq!((m.carried(), m.delivered(), m.lost()), (2, 1, 1));
        assert_eq!(eb.refused(), 1);
        assert!(!ea.has_inbound());
    }

    fn id(board: usize) -> EndpointId {
        EndpointId {
            board: ParticipantId(board),
            seam: "test",
        }
    }

    fn event(at: Cycles, bytes: &[u8]) -> EndpointEvent {
        EndpointEvent {
            at,
            bytes: bytes.to_vec(),
        }
    }
}
