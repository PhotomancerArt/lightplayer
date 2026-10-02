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
    /// Reliable messages accepted by `send`, per (incarnation, generation,
    /// channel).
    pub sent: BTreeMap<(u16, u32, u8), u32>,
    pub sent_count: u64,
    /// This side's session as its own events have reported it.
    pub rx_gen: u32,
    pub blocked: bool,
    pub peak_buffered: usize,
    pub peak_window: usize,
    pub peak_scratch: usize,
    /// Largest `Link::ram_bytes` seen, over every incarnation.
    pub peak_ram: usize,
    pub logs_written: u64,
    /// Log lines that died in the ring at a reboot.
    pub logs_lost_in_ring: u64,
    pub ups: u64,
    pub resets: u64,
    pub text_bytes: u64,
    /// Counters of earlier incarnations.
    pub past_counters: Vec<LinkCounters>,
    pub log_ring: LogRing<2048>,
    /// A secure link's edge (`Endpoint::new_secure`).
    #[cfg(feature = "secure")]
    pub secure: Option<crate::sim::secure_sim::SecureEdge>,
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
            peak_ram: 0,
            logs_written: 0,
            logs_lost_in_ring: 0,
            ups: 0,
            resets: 0,
            text_bytes: 0,
            past_counters: Vec::new(),
            log_ring: LogRing::new(),
            #[cfg(feature = "secure")]
            secure: None,
        }
    }

    /// An endpoint whose link is secure, in `edge`'s role.
    #[cfg(feature = "secure")]
    pub fn new_secure(
        cfg: LinkConfig,
        nonce: u32,
        edge: crate::sim::secure_sim::SecureEdge,
    ) -> Self {
        let mut e = Self::new(cfg.clone(), nonce);
        e.link = Link::new_secure(cfg, nonce, edge.role(), crate::sim::sim_entropy::fill);
        e.secure = Some(edge);
        e
    }

    /// The RAM bound this endpoint's link must stay under.
    pub fn ram_bound(&self) -> usize {
        #[cfg(feature = "secure")]
        if self.secure.is_some() {
            return Link::<A>::ram_bound_secure(&self.cfg);
        }
        Link::<A>::ram_bound(&self.cfg)
    }

    /// Power-cycle: a fresh link with a fresh nonce; nothing survives.
    pub fn reboot(&mut self, nonce: u32) {
        self.past_counters.push(self.link.counters().clone());
        self.link = Link::new(self.cfg.clone(), nonce);
        #[cfg(feature = "secure")]
        if let Some(edge) = self.secure.as_mut() {
            edge.reboot();
            self.link = Link::new_secure(
                self.cfg.clone(),
                nonce,
                edge.role(),
                crate::sim::sim_entropy::fill,
            );
        }
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
        let key = (self.inc, generation, channel);
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
        #[cfg(feature = "secure")]
        if let Some(edge) = self.secure.as_mut() {
            edge.service(&mut self.link);
        }
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
        self.peak_ram = self.peak_ram.max(self.link.ram_bytes());
    }

    /// Hand every event to the checker for the other direction.
    pub fn drain(&mut self, now: Micros, window_end: Micros, checker: &mut Checker) {
        self.peak_ram = self.peak_ram.max(self.link.ram_bytes());
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
                        let rx = (self.inc, self.rx_gen);
                        checker.on_reliable(now, window_end, rx, channel, &data);
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
            t = t.plus(c);
        }
        t
    }
}
