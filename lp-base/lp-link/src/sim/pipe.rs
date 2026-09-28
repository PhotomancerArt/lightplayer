//! One direction of a simulated transport, with fault injection.
//!
//! A *write* is what the link handed over (one frame). A stream pipe cuts it
//! into `packet`-byte transport packets (USB's 64-byte packets, BLE
//! notifications used as a byte pipe); a datagram pipe carries it as exactly
//! one packet. Packets leave either back to back at a byte rate, or only at
//! BLE connection events (a few per event). Faults act on packets.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};
use std::vec::Vec;

use crate::Micros;
use crate::sim::sim_rng::SimRng;

#[derive(Clone, Debug)]
pub struct PipeModel {
    pub name: &'static str,
    /// Bytes per transport packet. A datagram must fit one.
    pub packet: usize,
    /// One write = one packet (BLE notification, UDP datagram, WS message).
    pub datagram: bool,
    /// Serialization time per byte, in nanoseconds.
    pub byte_ns: u64,
    pub latency: Micros,
    /// Extra random delay per packet in `0..jitter` (reorders unless ordered).
    pub jitter: Micros,
    pub ordered: bool,
    /// BLE: packets leave only at connection events: (interval, per event).
    pub conn_events: Option<(Micros, usize)>,
    /// Packets queued in the sender before it pushes back.
    pub queue_packets: usize,
}

/// Probabilities, each per packet unless noted.
#[derive(Clone, Debug, Default)]
pub struct Faults {
    /// A whole packet lost.
    pub drop_packet: f64,
    /// The last packet of a write lost (the torn-tail shape seen on the C6).
    pub drop_tail: f64,
    /// A whole write lost (per write).
    pub drop_write: f64,
    /// A run of bytes inside a packet lost (head, middle or tail).
    pub drop_span: f64,
    /// One bit flipped.
    pub corrupt: f64,
    /// Delivered twice.
    pub duplicate: f64,
    /// Held back by `spike_len` (and, on an ordered pipe, everything after it).
    pub spike: f64,
    pub spike_len: Micros,
}

impl Faults {
    pub fn none() -> Self {
        Faults::default()
    }
}

#[derive(Clone, Debug, Default)]
pub struct PipeStats {
    pub writes: u64,
    pub packets: u64,
    pub bytes: u64,
    pub packets_lost: u64,
    pub spans_lost: u64,
    pub corrupted: u64,
    pub duplicated: u64,
}

pub struct Pipe {
    pub model: PipeModel,
    pub faults: Faults,
    /// Everything sent is lost (an outage).
    pub cut: bool,
    pub stats: PipeStats,
    rng: SimRng,
    wire_free_ns: u64,
    ev_time: Micros,
    ev_used: usize,
    backlog: VecDeque<Micros>,
    arrivals: BinaryHeap<Reverse<(Micros, u64, Vec<u8>)>>,
    last_arrival: Micros,
    order: u64,
}

impl Pipe {
    pub fn new(model: PipeModel, faults: Faults, rng: SimRng) -> Self {
        Pipe {
            model,
            faults,
            cut: false,
            stats: PipeStats::default(),
            rng,
            wire_free_ns: 0,
            ev_time: 0,
            ev_used: 0,
            backlog: VecDeque::new(),
            arrivals: BinaryHeap::new(),
            last_arrival: 0,
            order: 0,
        }
    }

    /// Room for another write.
    pub fn can_accept(&mut self, now: Micros) -> bool {
        while self.backlog.front().is_some_and(|&t| t <= now) {
            self.backlog.pop_front();
        }
        self.backlog.len() < self.model.queue_packets
    }

    /// When the next queued packet leaves (room may open then).
    pub fn accept_at(&self) -> Option<Micros> {
        self.backlog.front().copied()
    }

    pub fn send(&mut self, now: Micros, write: &[u8]) {
        self.stats.writes += 1;
        let drop_write = self.rng.chance(self.faults.drop_write);
        let chunks: Vec<&[u8]> = if self.model.datagram {
            assert!(
                write.len() <= self.model.packet,
                "{}: a {}-byte datagram exceeds the {}-byte packet",
                self.model.name,
                write.len(),
                self.model.packet
            );
            std::vec![write]
        } else {
            write.chunks(self.model.packet).collect()
        };
        let n = chunks.len();
        for (i, chunk) in chunks.into_iter().enumerate() {
            let depart = self.schedule(now, chunk.len());
            self.backlog.push_back(depart);
            self.stats.packets += 1;
            self.stats.bytes += chunk.len() as u64;
            let lost = drop_write
                || self.cut
                || self.rng.chance(self.faults.drop_packet)
                || (i + 1 == n && self.rng.chance(self.faults.drop_tail));
            if lost {
                self.stats.packets_lost += 1;
                continue;
            }
            let mut data = chunk.to_vec();
            if self.rng.chance(self.faults.drop_span) && !data.is_empty() {
                self.drop_span(&mut data);
            }
            if self.rng.chance(self.faults.corrupt) && !data.is_empty() {
                let bit = self.rng.below(data.len() as u64 * 8);
                data[(bit / 8) as usize] ^= 1 << (bit % 8);
                self.stats.corrupted += 1;
            }
            let mut at = depart + self.model.latency + self.rng.below(self.model.jitter + 1);
            if self.rng.chance(self.faults.spike) {
                at += self.faults.spike_len;
            }
            if self.model.ordered {
                at = at.max(self.last_arrival);
                self.last_arrival = at;
            }
            if self.rng.chance(self.faults.duplicate) {
                self.stats.duplicated += 1;
                let again = if self.model.ordered {
                    at
                } else {
                    at + self.rng.below(self.model.jitter + 1)
                };
                self.push_arrival(again, data.clone());
            }
            self.push_arrival(at, data);
        }
    }

    pub fn next_arrival(&self) -> Option<Micros> {
        self.arrivals.peek().map(|Reverse((t, _, _))| *t)
    }

    pub fn pop_arrival(&mut self, now: Micros) -> Option<Vec<u8>> {
        if self.next_arrival()? > now {
            return None;
        }
        self.arrivals.pop().map(|Reverse((_, _, d))| d)
    }

    fn push_arrival(&mut self, at: Micros, data: Vec<u8>) {
        self.order += 1;
        self.arrivals.push(Reverse((at, self.order, data)));
    }

    /// When a packet of `len` bytes handed over at `now` leaves.
    fn schedule(&mut self, now: Micros, len: usize) -> Micros {
        match self.model.conn_events {
            None => {
                let start = self.wire_free_ns.max(now * 1_000);
                self.wire_free_ns = start + len as u64 * self.model.byte_ns;
                self.wire_free_ns.div_ceil(1_000)
            }
            Some((interval, per_event)) => {
                if self.ev_time < now {
                    self.ev_time = now.div_ceil(interval) * interval;
                    self.ev_used = 0;
                }
                if self.ev_used >= per_event {
                    self.ev_time += interval;
                    self.ev_used = 0;
                }
                self.ev_used += 1;
                self.ev_time
            }
        }
    }

    fn drop_span(&mut self, data: &mut Vec<u8>) {
        let len = data.len() as u64;
        let span = 1 + self.rng.below(len.div_ceil(2));
        let start = match self.rng.below(3) {
            0 => 0,
            1 => len - span,
            _ => self.rng.below(len - span + 1),
        } as usize;
        data.drain(start..start + span as usize);
        self.stats.spans_lost += 1;
    }
}
