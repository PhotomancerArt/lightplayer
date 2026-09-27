//! The link: one end of a session over any transport, sans-IO.
//!
//! The edge (an embassy task on the board, a Web Serial pump in the page,
//! the simulator in tests) owns the transport and the clock and drives it:
//!
//! ```text
//! bytes in    → link.on_bytes(now, bytes)      (stream framing)
//! datagram in → link.on_datagram(now, frame)   (datagram framing)
//! while let Some(frame) = link.poll_transmit(now) { write frame }
//! sleep until link.poll_timeout(), new input, or a send()
//! app:  link.send(channel, bytes)  /  link.recv() -> LinkEvent
//! ```
//!
//! Lifecycle: each end picks a random nonce. SYNs carry "my nonce, your nonce
//! as I know it"; the link is up once each side has seen the other name it.
//! Every restart picks a new nonce, so the other end sees a SYN with a new
//! nonce, resets too, and both report [`LinkEvent::Reset`]. Frames are
//! checksummed under a key derived from both nonces, so a frame from an older
//! session can never pass for a current one.

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::mem;

use crate::Micros;
use crate::arq::{Arq, Feedback, RxVerdict};
use crate::cobs;
use crate::deframer::{Deframed, Deframer, IdleFlush};
use crate::frame::{self, FrameKind, HEADER_LEN, Header, SACK_LEN, SYN_LEN, SynBody};
use crate::inbox::{Fragment, Inbox};
use crate::link_config::{Framing, LinkConfig};
use crate::link_counters::LinkCounters;
use crate::link_event::{LinkEvent, ResetReason};
use crate::log_ring::LogRing;
use crate::rtt_estimator::RttEstimator;
use crate::seq_num::seq_dist;
use crate::tx_queue::{TxEntry, TxQueue};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState {
    /// Handshaking: sending SYNs, no data moves.
    Connecting,
    Established,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendError {
    /// The send budget (or the datagram queue) is full; try after `recv`/
    /// `poll_transmit` have made progress.
    Full,
    /// Longer than `max_message` (reliable) or one frame (best effort).
    TooBig,
    BadChannel,
}

pub struct Link<A: Arq> {
    cfg: LinkConfig,
    state: LinkState,
    nonce: u32,
    peer_nonce: Option<u32>,
    prev_key: Option<u32>,
    peer_max_payload: u16,
    peer_rx_window: u8,
    generation: u32,
    arq: A,
    tx: TxQueue,
    inbox: Inbox,
    rtt: RttEstimator,
    backoff: u8,
    send_order: u32,
    pending: VecDeque<(u8, Vec<u8>)>,
    pending_off: usize,
    pending_bytes: usize,
    datagrams: VecDeque<(u8, Vec<u8>)>,
    deframer: Deframer,
    rx_raw: Vec<u8>,
    raw: Vec<u8>,
    out: Vec<u8>,
    tx_payload: usize,
    peer_win: u8,
    ack_due: Option<Micros>,
    ack_explicit: bool,
    ack_trigger: Option<u8>,
    unacked_rx: u8,
    last_adv_win: u8,
    syn_due: Option<Micros>,
    syn_owed: bool,
    last_tx: Micros,
    last_rx: Micros,
    counters: LinkCounters,
}

impl<A: Arq> Link<A> {
    /// A new link. `nonce` must be random per boot / page load (it is what
    /// tells the peer we restarted).
    pub fn new(cfg: LinkConfig, nonce: u32) -> Self {
        let body_max = (cfg.max_payload as usize).max(SYN_LEN);
        let max_frame = cobs::max_encoded_len(HEADER_LEN + body_max + cfg.crc.len());
        let rx_window = cfg.rx_window.min(A::MAX_WINDOW);
        Link {
            state: LinkState::Connecting,
            nonce: nonce.max(1),
            peer_nonce: None,
            prev_key: None,
            peer_max_payload: 0,
            peer_rx_window: 0,
            generation: 0,
            arq: A::new(rx_window),
            tx: TxQueue::default(),
            inbox: Inbox::new(cfg.rx_budget, cfg.max_message),
            rtt: RttEstimator::new(cfg.initial_rto, cfg.min_rto, cfg.max_rto),
            backoff: 0,
            send_order: 0,
            pending: VecDeque::new(),
            pending_off: 0,
            pending_bytes: 0,
            datagrams: VecDeque::new(),
            deframer: Deframer::new(max_frame),
            rx_raw: Vec::new(),
            raw: Vec::new(),
            out: Vec::new(),
            tx_payload: 0,
            peer_win: 0,
            ack_due: None,
            ack_explicit: false,
            ack_trigger: None,
            unacked_rx: 0,
            last_adv_win: 0,
            syn_due: Some(0),
            syn_owed: false,
            last_tx: 0,
            last_rx: 0,
            counters: LinkCounters::default(),
            cfg,
        }
    }

    // ---- Application side ------------------------------------------------

    /// Queue a message. Reliable channels take up to `max_message` bytes and
    /// fragment; best-effort channels take one frame's worth.
    pub fn send(&mut self, channel: u8, payload: &[u8]) -> Result<(), SendError> {
        if channel >= 8 {
            return Err(SendError::BadChannel);
        }
        if self.cfg.is_reliable(channel) {
            if payload.len() > self.cfg.max_message {
                return Err(SendError::TooBig);
            }
            if self.pending_bytes + self.tx.bytes() + payload.len() > self.cfg.send_budget {
                return Err(SendError::Full);
            }
            self.pending_bytes += payload.len();
            self.pending.push_back((channel, payload.to_vec()));
        } else {
            if payload.len() > self.cfg.max_payload as usize {
                return Err(SendError::TooBig);
            }
            if self.datagrams.len() >= self.cfg.datagram_queue {
                self.counters.datagrams_dropped += 1;
                return Err(SendError::Full);
            }
            self.datagrams.push_back((channel, payload.to_vec()));
        }
        Ok(())
    }

    /// Move log records from `ring` into the log channel while there is room.
    /// Records stay in the ring (which drops its oldest) while the link is
    /// down, so logging stays cheap and bounded then.
    pub fn pump_log<const N: usize>(&mut self, ring: &mut LogRing<N>, channel: u8) {
        if self.state != LinkState::Established {
            return;
        }
        let mut rec = [0u8; 256];
        while self.datagrams.len() < self.cfg.datagram_queue {
            let limit = rec.len().min(self.tx_payload);
            let Some(n) = ring.pop_into(&mut rec[..limit]) else {
                break;
            };
            self.datagrams.push_back((channel, rec[..n].to_vec()));
        }
    }

    /// The next event: a message, text, link up, or reset.
    pub fn recv(&mut self) -> Option<LinkEvent> {
        let ev = self.inbox.pop()?;
        match self.arq.drain(&mut self.inbox) {
            Ok(0) => {}
            Ok(_) => self.ack_due = Some(0),
            Err(_) => self.protocol_error(),
        }
        if self.state == LinkState::Established && self.last_adv_win == 0 && self.adv_window() > 0 {
            self.ack_due = Some(0);
        }
        Some(ev)
    }

    /// Drop the session and start a new one (a new nonce).
    pub fn restart(&mut self, now: Micros) {
        self.reset(now, ResetReason::Requested);
    }

    // ---- Transport side --------------------------------------------------

    /// Bytes from a stream transport ([`Framing::Stream`]).
    pub fn on_bytes(&mut self, now: Micros, bytes: &[u8]) {
        self.counters.bytes_rx += bytes.len() as u64;
        for &b in bytes {
            match self.deframer.push(now, b) {
                Deframed::Nothing => {}
                Deframed::Text => self.flush_text(),
                Deframed::Overflow => self.counters.oversize_frames += 1,
                Deframed::Frame => {
                    let mut raw = mem::take(&mut self.rx_raw);
                    raw.clear();
                    let ok = match cobs::decode_into(self.deframer.frame(), &mut raw) {
                        Ok(()) => self.on_frame(now, &raw),
                        Err(_) => {
                            self.counters.bad_frames += 1;
                            false
                        }
                    };
                    self.deframer.frame_done(ok);
                    self.rx_raw = raw;
                }
            }
        }
    }

    /// One whole frame from a datagram transport ([`Framing::Datagram`]).
    pub fn on_datagram(&mut self, now: Micros, frame: &[u8]) {
        self.counters.bytes_rx += frame.len() as u64;
        self.on_frame(now, frame);
    }

    /// The next frame to write, if any. Call until `None` whenever the
    /// transport can take more. Each returned slice is one whole frame (write
    /// it as one datagram, or as bytes on a stream).
    pub fn poll_transmit(&mut self, now: Micros) -> Option<&[u8]> {
        self.service_timers(now);
        let mut sent = self.pick_and_emit(now);
        if !sent && self.state == LinkState::Connecting {
            // A reset inside pick_and_emit leaves a SYN due now.
            sent = self.pick_and_emit(now);
        }
        if !sent {
            return None;
        }
        self.last_tx = now;
        self.counters.frames_tx += 1;
        self.counters.bytes_tx += self.out.len() as u64;
        Some(&self.out)
    }

    /// When the link next needs `poll_transmit` for a timer (retransmit,
    /// delayed ACK, keepalive, SYN, idle flush). New input and `send()` need a
    /// `poll_transmit` too; this does not cover them.
    pub fn poll_timeout(&self) -> Option<Micros> {
        let mut t = self.deframer.idle_deadline(self.cfg.idle_flush);
        let mut min = |x: Option<Micros>| {
            if let Some(x) = x {
                t = Some(t.map_or(x, |t| t.min(x)));
            }
        };
        match self.state {
            LinkState::Connecting => min(self.syn_due),
            LinkState::Established => {
                min(self.ack_due);
                if A::RELIABLE {
                    min(self.tx.next_timer(self.rto()));
                }
                min(Some(self.last_tx + self.cfg.keepalive));
            }
        }
        t
    }

    // ---- Introspection ---------------------------------------------------

    pub fn state(&self) -> LinkState {
        self.state
    }

    /// Session number: bumped on every reset.
    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub fn counters(&self) -> &LinkCounters {
        &self.counters
    }

    pub fn config(&self) -> &LinkConfig {
        &self.cfg
    }

    /// How long since anything verified arrived.
    pub fn peer_silent_for(&self, now: Micros) -> Micros {
        now.saturating_sub(self.last_rx)
    }

    /// Nothing queued, nothing unacknowledged.
    pub fn is_idle(&self) -> bool {
        self.pending.is_empty() && self.tx.is_empty() && self.datagrams.is_empty()
    }

    /// Payload bytes held right now: queued to send, unacknowledged, held out
    /// of order, reassembling, and waiting for `recv()`.
    pub fn buffered_bytes(&self) -> usize {
        self.pending_bytes
            + self.tx.bytes()
            + self.datagrams.iter().map(|(_, d)| d.len()).sum::<usize>()
            + self.arq.reorder_bytes()
            + self.inbox.bytes()
    }

    /// Payload bytes the reliability machinery holds: sent but unacknowledged,
    /// plus received out of order. What the windows cost in RAM.
    pub fn window_bytes(&self) -> usize {
        self.tx.bytes() + self.arq.reorder_bytes()
    }

    /// Fixed scratch capacity (frame buffers).
    pub fn scratch_bytes(&self) -> usize {
        self.raw.capacity()
            + self.out.capacity()
            + self.rx_raw.capacity()
            + self.deframer.capacity()
    }

    pub fn rto(&self) -> Micros {
        (self.rtt.rto() << self.backoff).min(self.cfg.max_rto)
    }

    // ---- Receive path ----------------------------------------------------

    /// Handle one decoded frame; `true` if it verified (under any key).
    fn on_frame(&mut self, now: Micros, raw: &[u8]) -> bool {
        let crc = self.cfg.crc;
        let Some(hdr) = Header::parse(raw) else {
            self.counters.bad_frames += 1;
            return false;
        };
        if hdr.kind == FrameKind::Syn {
            let Some(syn) = frame::verify(crc, 0, raw).and_then(SynBody::parse) else {
                self.counters.bad_frames += 1;
                return false;
            };
            self.counters.frames_rx += 1;
            self.last_rx = now;
            self.on_syn(now, syn);
            return true;
        }
        let Some(peer) = self.peer_nonce else {
            self.counters.dropped_unsynced += 1;
            return false;
        };
        let Some(body) = frame::verify(crc, self.nonce ^ peer, raw) else {
            if self
                .prev_key
                .is_some_and(|k| frame::verify(crc, k, raw).is_some())
            {
                self.counters.stale_frames += 1;
                return true;
            }
            self.counters.bad_frames += 1;
            return false;
        };
        self.counters.frames_rx += 1;
        self.last_rx = now;
        if self.state == LinkState::Connecting {
            // Only a peer that knows our current nonce can key a frame so.
            self.establish(now);
        }
        match hdr.kind {
            FrameKind::Data => {
                self.on_ack_fields(now, &hdr, 0, None);
                self.on_data(now, &hdr, body);
            }
            FrameKind::Datagram => {
                self.on_ack_fields(now, &hdr, 0, None);
                if self.inbox.has_room(body.len()) {
                    self.inbox.push_datagram(hdr.chan, body);
                } else {
                    self.counters.datagrams_dropped += 1;
                }
            }
            FrameKind::Ack => {
                let sack = match body.len() {
                    SACK_LEN => u32::from_le_bytes([body[0], body[1], body[2], body[3]]),
                    _ => 0,
                };
                self.on_ack_fields(now, &hdr, sack, hdr.fin.then_some(hdr.seq));
            }
            FrameKind::Syn => {}
        }
        true
    }

    fn on_syn(&mut self, now: Micros, syn: SynBody) {
        self.peer_max_payload = syn.max_payload;
        self.peer_rx_window = syn.rx_window;
        match self.state {
            LinkState::Established if Some(syn.nonce) != self.peer_nonce => {
                self.reset(now, ResetReason::PeerRestarted);
                self.peer_nonce = Some(syn.nonce);
            }
            LinkState::Established => {
                if syn.your != self.nonce {
                    // A stale SYN, or a peer that learned an old nonce of ours:
                    // telling it again costs one frame and cannot deadlock.
                    self.counters.stale_syns += 1;
                    self.syn_owed = true;
                } else if !syn.established {
                    self.syn_owed = true;
                }
            }
            LinkState::Connecting => {
                self.peer_nonce = Some(syn.nonce);
                if syn.your == self.nonce {
                    self.establish(now);
                    if !syn.established {
                        self.syn_owed = true;
                    }
                } else {
                    self.syn_due = Some(now);
                }
            }
        }
    }

    fn on_data(&mut self, now: Micros, hdr: &Header, body: &[u8]) {
        let frag = Fragment {
            chan: hdr.chan,
            first: hdr.first,
            fin: hdr.fin,
            data: body,
        };
        match self.arq.on_data(hdr.seq, frag, &mut self.inbox) {
            RxVerdict::InOrder => {
                self.unacked_rx = self.unacked_rx.saturating_add(1);
                let due = if self.unacked_rx >= self.cfg.ack_every {
                    now
                } else {
                    now + self.cfg.ack_delay
                };
                self.ack_due = Some(self.ack_due.map_or(due, |t| t.min(due)));
            }
            RxVerdict::Buffered | RxVerdict::Gap => {
                self.counters.out_of_order += 1;
                self.ack_explicit = true;
                self.ack_trigger = Some(hdr.seq);
                self.ack_due = Some(now);
            }
            RxVerdict::Duplicate => {
                self.counters.duplicates += 1;
                self.ack_due = Some(now);
            }
            RxVerdict::NoRoom => {
                self.counters.rx_no_room += 1;
                self.ack_due = Some(now);
            }
            RxVerdict::OutOfWindow => self.counters.out_of_order += 1,
            RxVerdict::Protocol => self.protocol_error(),
        }
        if !A::RELIABLE {
            self.ack_due = None;
            self.ack_explicit = false;
        }
    }

    fn on_ack_fields(&mut self, now: Micros, hdr: &Header, sack: u32, trigger: Option<u8>) {
        if !A::RELIABLE {
            return;
        }
        let Some(acked) = self.tx.ack_to(hdr.ack, now) else {
            return;
        };
        if acked.frames > 0 {
            self.backoff = 0;
        }
        if let Some(r) = acked.rtt_sample {
            self.rtt.sample(r);
        }
        self.peer_win = hdr.win;
        let fb = Feedback {
            sack,
            trigger,
            now,
            srtt: self.rtt.srtt(),
        };
        self.counters.fast_retransmits += A::on_feedback(&mut self.tx, fb) as u32;
    }

    fn flush_text(&mut self) {
        let text = self.deframer.take_text();
        self.counters.text_bytes += text.len() as u32;
        self.inbox.push_event(LinkEvent::Text(text));
    }

    // ---- Transmit path ---------------------------------------------------

    fn service_timers(&mut self, now: Micros) {
        if self
            .deframer
            .idle_deadline(self.cfg.idle_flush)
            .is_some_and(|t| t <= now)
        {
            match self.deframer.flush_idle() {
                IdleFlush::Text => self.flush_text(),
                IdleFlush::Garbage => self.counters.stale_partials += 1,
                IdleFlush::Nothing => {}
            }
        }
        if self.state == LinkState::Established && A::RELIABLE {
            let rto = self.rto();
            if self.tx.next_timer(rto).is_some_and(|t| t <= now) {
                A::on_timeout(&mut self.tx, now, rto);
                self.counters.timeouts += 1;
                self.backoff = (self.backoff + 1).min(8);
            }
        }
    }

    /// Choose the next frame and encode it into `out`.
    fn pick_and_emit(&mut self, now: Micros) -> bool {
        if self.state == LinkState::Connecting {
            if self.syn_due.is_some_and(|t| t <= now) {
                self.syn_due = Some(now + self.cfg.syn_interval);
                self.emit_syn();
                return true;
            }
            return false;
        }
        if self.syn_owed {
            self.syn_owed = false;
            self.emit_syn();
            return true;
        }
        let ack_ready = self.ack_due.is_some_and(|t| t <= now);
        if ack_ready && self.ack_explicit {
            self.emit_ack();
            return true;
        }
        if let Some(seq) = self.tx.first_unsent() {
            if self
                .tx
                .get(seq)
                .is_some_and(|e| e.sends > self.cfg.max_retries)
            {
                self.reset(now, ResetReason::RetryLimit);
                return false;
            }
            self.emit_data(now, seq);
            return true;
        }
        if self.can_send_new()
            && let Some(entry) = self.next_fragment()
        {
            let seq = self.tx.push(entry);
            self.emit_data(now, seq);
            return true;
        }
        while let Some((chan, data)) = self.datagrams.pop_front() {
            if data.len() > self.tx_payload {
                self.counters.datagrams_dropped += 1;
                continue;
            }
            let hdr = self.header(FrameKind::Datagram, chan);
            self.encode(&hdr, &data);
            return true;
        }
        if ack_ready || now >= self.last_tx + self.cfg.keepalive {
            self.emit_ack();
            return true;
        }
        false
    }

    fn can_send_new(&self) -> bool {
        if !A::RELIABLE {
            return true;
        }
        let window = self.cfg.tx_window.min(A::MAX_WINDOW) as usize;
        self.tx.len() < window
            && (seq_dist(self.tx.base(), self.tx.next_seq()) as usize) < self.peer_win as usize
    }

    fn next_fragment(&mut self) -> Option<TxEntry> {
        let (chan, msg) = self.pending.front()?;
        let start = self.pending_off;
        let end = (start + self.tx_payload).min(msg.len());
        let entry = TxEntry::new(
            *chan,
            start == 0,
            end == msg.len(),
            msg[start..end].to_vec(),
        );
        if entry.fin {
            self.pending_bytes -= msg.len();
            self.pending.pop_front();
            self.pending_off = 0;
        } else {
            self.pending_off = end;
        }
        Some(entry)
    }

    fn emit_data(&mut self, now: Micros, seq: u8) {
        let mut hdr = self.header(FrameKind::Data, 0);
        self.send_order = self.send_order.wrapping_add(1);
        let Some(e) = self.tx.get_mut(seq) else {
            return;
        };
        e.sent_at = Some(now);
        e.sent_order = self.send_order;
        e.sends = e.sends.saturating_add(1);
        if e.sends > 1 {
            self.counters.retransmits += 1;
        }
        self.counters.data_frames_tx += 1;
        hdr.chan = e.chan;
        hdr.first = e.first;
        hdr.fin = e.fin;
        hdr.seq = seq;
        let key = self.nonce ^ self.peer_nonce.unwrap_or(0);
        frame::encode_raw(self.cfg.crc, key, &hdr, &e.payload, &mut self.raw);
        finish(self.cfg.framing, &mut self.raw, &mut self.out);
        if !A::RELIABLE {
            self.tx.pop_front();
        }
    }

    fn emit_ack(&mut self) {
        let mut hdr = self.header(FrameKind::Ack, 0);
        if let Some(t) = self.ack_trigger.take() {
            hdr.fin = true;
            hdr.seq = t;
        }
        self.ack_explicit = false;
        let sack = self.arq.sack();
        let body = sack.to_le_bytes();
        self.encode(&hdr, if sack != 0 { &body } else { &[] });
    }

    fn emit_syn(&mut self) {
        let body = SynBody {
            nonce: self.nonce,
            your: self.peer_nonce.unwrap_or(0),
            established: self.state == LinkState::Established,
            max_payload: self.cfg.max_payload,
            rx_window: self.adv_window(),
        };
        let hdr = Header {
            kind: FrameKind::Syn,
            fin: false,
            first: false,
            chan: 0,
            seq: 0,
            ack: 0,
            win: 0,
        };
        frame::encode_raw(self.cfg.crc, 0, &hdr, &body.to_bytes(), &mut self.raw);
        finish(self.cfg.framing, &mut self.raw, &mut self.out);
    }

    /// A header carrying the current ACK and window. Sending it satisfies any
    /// plain (non-explicit) ACK owed.
    fn header(&mut self, kind: FrameKind, chan: u8) -> Header {
        let win = self.adv_window();
        self.last_adv_win = win;
        if !self.ack_explicit || kind == FrameKind::Ack {
            self.ack_due = None;
            self.unacked_rx = 0;
        }
        Header {
            kind,
            fin: false,
            first: false,
            chan,
            seq: 0,
            ack: self.arq.expected(),
            win,
        }
    }

    fn encode(&mut self, hdr: &Header, body: &[u8]) {
        let key = self.key();
        frame::encode_raw(self.cfg.crc, key, hdr, body, &mut self.raw);
        finish(self.cfg.framing, &mut self.raw, &mut self.out);
    }

    /// Frames past our ACK we can take now.
    fn adv_window(&self) -> u8 {
        let used = self.inbox.ready_bytes() + self.arq.reorder_bytes();
        let free =
            self.inbox.budget().saturating_sub(used) / (self.cfg.max_payload as usize).max(1);
        free.min(self.cfg.rx_window.min(A::MAX_WINDOW) as usize) as u8
    }

    fn key(&self) -> u32 {
        self.nonce ^ self.peer_nonce.unwrap_or(0)
    }

    // ---- Lifecycle -------------------------------------------------------

    fn establish(&mut self, now: Micros) {
        self.state = LinkState::Established;
        self.tx_payload = self.cfg.max_payload.min(self.peer_max_payload).max(1) as usize;
        self.peer_win = self.peer_rx_window;
        self.last_rx = now;
        self.last_tx = now;
        self.counters.ups += 1;
        self.inbox.push_event(LinkEvent::Up {
            generation: self.generation,
        });
    }

    fn reset(&mut self, now: Micros, reason: ResetReason) {
        if self.state == LinkState::Established {
            self.prev_key = Some(self.key());
        }
        self.generation = self.generation.wrapping_add(1);
        self.nonce = next_nonce(self.nonce);
        self.state = LinkState::Connecting;
        self.arq.reset();
        self.tx.clear();
        self.pending.clear();
        self.pending_off = 0;
        self.pending_bytes = 0;
        self.datagrams.clear();
        self.inbox.abort_partial();
        self.backoff = 0;
        self.peer_win = 0;
        self.ack_due = None;
        self.ack_explicit = false;
        self.ack_trigger = None;
        self.unacked_rx = 0;
        self.syn_owed = false;
        self.syn_due = Some(now);
        self.counters.resets += 1;
        self.inbox.push_event(LinkEvent::Reset {
            reason,
            generation: self.generation,
        });
    }

    fn protocol_error(&mut self) {
        self.counters.protocol_errors += 1;
        if A::RELIABLE {
            let now = self.last_rx;
            self.reset(now, ResetReason::ProtocolError);
        }
    }
}

/// Turn the raw frame in `raw` into what goes on the transport, in `out`.
fn finish(framing: Framing, raw: &mut Vec<u8>, out: &mut Vec<u8>) {
    match framing {
        Framing::Stream => frame::wrap_stream(raw, out),
        // header ‖ body ‖ crc is exactly one datagram.
        Framing::Datagram => mem::swap(raw, out),
    }
}

/// The nonce after a restart: a bijective mix, never 0 ("unknown" on the wire).
fn next_nonce(n: u32) -> u32 {
    let mut z = n.wrapping_add(0x9E37_79B9);
    z = (z ^ (z >> 16)).wrapping_mul(0x85EB_CA6B);
    z = (z ^ (z >> 13)).wrapping_mul(0xC2B2_AE35);
    (z ^ (z >> 16)).max(1)
}
