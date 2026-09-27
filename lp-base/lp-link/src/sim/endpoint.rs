//! One side of a simulated run: a link, what it has sent, and the glue a
//! real edge would be (feeding bytes in, draining frames out).

use std::collections::BTreeMap;
use std::format;
use std::vec::Vec;

use crate::log_ring::{LEVEL_INFO, LogRing};
use crate::sim::checker::Checker;
use crate::sim::pipe::Pipe;
use crate::sim::probe_message::Probe;
use crate::{Arq, CH_LOG, Framing, Link, LinkConfig, LinkCounters, LinkEvent, Micros, SendError};

pub struct Endpoint<A: Arq> {
    pub link: Link<A>,
    pub cfg: LinkConfig,
    /// Incarnation: bumped by a reboot.
    pub inc: u16,
    /// Reliable messages accepted by `send`, per (incarnation, generation).
    pub sent: BTreeMap<(u16, u32), u32>,
    pub sent_count: u64,
    /// This side's session as its own events have reported it.
    pub rx_gen: u32,
    pub blocked: bool,
    pub peak_buffered: usize,
    pub peak_window: usize,
    pub peak_scratch: usize,
    pub logs_written: u64,
    /// Log lines that died in the ring at a reboot.
    pub logs_lost_in_ring: u64,
    pub ups: u64,
    pub resets: u64,
    pub text_bytes: u64,
    /// Counters of earlier incarnations.
    pub past_counters: Vec<LinkCounters>,
    pub log_ring: LogRing<2048>,
}

impl<A: Arq> Endpoint<A> {
    pub fn new(cfg: LinkConfig, nonce: u32) -> Self {
        Endpoint {
            link: Link::new(cfg.clone(), nonce),
            cfg,
            inc: 0,
            sent: BTreeMap::new(),
            sent_count: 0,
            rx_gen: 0,
            blocked: false,
            peak_buffered: 0,
            peak_window: 0,
            peak_scratch: 0,
            logs_written: 0,
            logs_lost_in_ring: 0,
            ups: 0,
            resets: 0,
            text_bytes: 0,
            past_counters: Vec::new(),
            log_ring: LogRing::new(),
        }
    }

    /// Power-cycle: a fresh link with a fresh nonce; nothing survives.
    pub fn reboot(&mut self, nonce: u32) {
        self.past_counters.push(self.link.counters().clone());
        self.link = Link::new(self.cfg.clone(), nonce);
        self.inc += 1;
        self.rx_gen = 0;
        self.logs_lost_in_ring += self.log_ring.len() as u64;
        self.log_ring = LogRing::new();
    }

    pub fn feed(&mut self, now: Micros, data: &[u8]) {
        match self.cfg.framing {
            Framing::Stream => self.link.on_bytes(now, data),
            Framing::Datagram => self.link.on_datagram(now, data),
        }
    }

    /// Offer a reliable probe message; `false` if the link refused it.
    pub fn send_probe(&mut self, now: Micros, channel: u8, size: usize) -> bool {
        let generation = self.link.generation();
        let key = (self.inc, generation);
        let idx = *self.sent.get(&key).unwrap_or(&0);
        let p = Probe {
            inc: self.inc,
            gen_: generation,
            idx,
            sent_at: now,
        };
        match self.link.send(channel, &p.encode(size)) {
            Ok(()) => {
                self.sent.insert(key, idx + 1);
                self.sent_count += 1;
                true
            }
            Err(SendError::Full) => false,
            Err(e) => panic!("send refused: {e:?}"),
        }
    }

    pub fn log(&mut self, now: Micros) {
        self.logs_written += 1;
        let line = format!("t={now} inc={} a log line from the board", self.inc);
        self.log_ring.push(LEVEL_INFO, line.as_bytes());
    }

    /// Write frames while the pipe takes them.
    pub fn service(&mut self, now: Micros, pipe: &mut Pipe) {
        self.link.pump_log(now, &mut self.log_ring, CH_LOG);
        self.blocked = false;
        loop {
            if !pipe.can_accept(now) {
                self.blocked = true;
                break;
            }
            match self.link.poll_transmit(now) {
                Some(frame) => pipe.send(now, frame),
                None => break,
            }
        }
        self.peak_buffered = self.peak_buffered.max(self.link.buffered_bytes());
        self.peak_window = self.peak_window.max(self.link.window_bytes());
        self.peak_scratch = self.peak_scratch.max(self.link.scratch_bytes());
    }

    /// Hand every event to the checker for the other direction.
    pub fn drain(&mut self, now: Micros, window_end: Micros, checker: &mut Checker) {
        while let Some(ev) = self.link.recv() {
            match ev {
                LinkEvent::Up { generation } => {
                    self.rx_gen = generation;
                    self.ups += 1;
                }
                LinkEvent::Reset { generation, .. } => {
                    self.rx_gen = generation;
                    self.resets += 1;
                }
                LinkEvent::Message { channel, data } => {
                    if self.cfg.is_reliable(channel) {
                        checker.on_reliable(now, window_end, (self.inc, self.rx_gen), &data);
                    } else {
                        checker.on_log(&data);
                    }
                }
                LinkEvent::Text(t) => self.text_bytes += t.len() as u64,
            }
        }
    }

    /// When this side next needs servicing.
    pub fn wake(&self, pipe: &Pipe) -> Option<Micros> {
        if self.blocked {
            pipe.accept_at()
        } else {
            self.link.poll_timeout()
        }
    }

    /// Counters summed over every incarnation.
    pub fn total_counters(&self) -> LinkCounters {
        let mut t = self.link.counters().clone();
        for c in &self.past_counters {
            add_counters(&mut t, c);
        }
        t
    }
}

fn add_counters(t: &mut LinkCounters, c: &LinkCounters) {
    t.frames_tx += c.frames_tx;
    t.frames_rx += c.frames_rx;
    t.bytes_tx += c.bytes_tx;
    t.bytes_rx += c.bytes_rx;
    t.data_frames_tx += c.data_frames_tx;
    t.retransmits += c.retransmits;
    t.timeouts += c.timeouts;
    t.fast_retransmits += c.fast_retransmits;
    t.probes += c.probes;
    t.bad_frames += c.bad_frames;
    t.stale_frames += c.stale_frames;
    t.oversize_frames += c.oversize_frames;
    t.dropped_unsynced += c.dropped_unsynced;
    t.duplicates += c.duplicates;
    t.out_of_order += c.out_of_order;
    t.rx_no_room += c.rx_no_room;
    t.datagrams_dropped += c.datagrams_dropped;
    t.datagrams_lost += c.datagrams_lost;
    t.stale_partials += c.stale_partials;
    t.text_bytes += c.text_bytes;
    t.ups += c.ups;
    t.resets += c.resets;
    t.stale_syns += c.stale_syns;
    t.protocol_errors += c.protocol_errors;
}
