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

use alloc::vec::Vec;
use core::mem;

use crate::Micros;
use crate::arq::{Arq, Feedback, RxVerdict};
use crate::cobs;
use crate::datagram_queue::DatagramQueue;
use crate::deframer::{Deframed, Deframer, IdleFlush};
use crate::frame::{self, FrameKind, HEADER_LEN, Header, SACK_LEN, SYN_LEN, SynBody};
use crate::inbox::{EVENT_COST, Fragment, Inbox};
use crate::link_config::{Framing, LinkConfig};
use crate::link_counters::LinkCounters;
use crate::link_event::{LinkEvent, ResetReason};
use crate::log_ring::LogRing;
use crate::rtt_estimator::RttEstimator;
use crate::send_queue::SendQueue;
use crate::seq_num::seq_dist;
use crate::tx_queue::TxQueue;

#[cfg(feature = "secure")]
mod sealed_frames;
#[cfg(feature = "secure")]
mod secure_handshake;
#[cfg(feature = "secure")]
mod secure_state;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState {
    /// Handshaking: sending SYNs, no data moves.
    Connecting,
    Established,
}

/// [`Link::cancel_external`]: the external message's first fragment already
/// went out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExternalStarted;

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
    pending: SendQueue,
    datagrams: DatagramQueue,
    /// Reliable data frames sent since the last datagram (`datagram_every`).
    data_run: u8,
    dgram_tx_seq: u8,
    dgram_rx_next: Option<u8>,
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
    /// Unanswered SYNs the gap has doubled for so far (`syn_backoff`).
    syn_doublings: u8,
    syn_owed: bool,
    /// A tail-loss probe may fire for the current flight.
    probe_armed: bool,
    last_data_tx: Micros,
    last_tx: Micros,
    last_rx: Micros,
    counters: LinkCounters,
    /// RAM allocated in `new` and never resized (see [`Link::ram_bound`]).
    fixed_ram: usize,
    /// A secure link's handshake, keys and counters (`Link::new_secure`);
    /// `None` on a plain link, which then behaves exactly as without the
    /// feature.
    #[cfg(feature = "secure")]
    secure: Option<alloc::boxed::Box<secure_state::SecureState>>,
}

impl<A: Arq> Link<A> {
    /// A new link. `nonce` must be random per boot / page load (it is what
    /// tells the peer we restarted).
    pub fn new(cfg: LinkConfig, nonce: u32) -> Self {
        let shape = Shape::of::<A>(&cfg);
        let rx_window = cfg.rx_window.min(A::MAX_WINDOW);
        let max_payload = cfg.max_payload as usize;
        Link {
            state: LinkState::Connecting,
            nonce: nonce.max(1),
            peer_nonce: None,
            prev_key: None,
            peer_max_payload: 0,
            peer_rx_window: 0,
            generation: 0,
            arq: A::new(rx_window, max_payload),
            tx: TxQueue::new(shape.tx_window, max_payload),
            inbox: Inbox::new(cfg.rx_budget, cfg.max_message, cfg.keep_reassembly),
            rtt: RttEstimator::new(cfg.initial_rto, cfg.min_rto, cfg.max_rto),
            backoff: 0,
            send_order: 0,
            pending: SendQueue::new(cfg.send_budget, cfg.send_queue),
            datagrams: DatagramQueue::new(cfg.datagram_queue, max_payload),
            data_run: 0,
            dgram_tx_seq: 0,
            dgram_rx_next: None,
            deframer: Deframer::new(shape.max_cobs, cfg.framing == Framing::Stream)
                .with_text_mark(cfg.escape_ff && cfg.framing == Framing::Stream),
            rx_raw: Vec::with_capacity(shape.max_rx_raw),
            raw: Vec::with_capacity(shape.max_wire),
            out: Vec::with_capacity(shape.max_wire),
            tx_payload: 0,
            peer_win: 0,
            ack_due: None,
            ack_explicit: false,
            ack_trigger: None,
            unacked_rx: 0,
            last_adv_win: 0,
            syn_due: Some(0),
            syn_doublings: 0,
            syn_owed: false,
            probe_armed: false,
            last_data_tx: 0,
            last_tx: 0,
            last_rx: 0,
            counters: LinkCounters::default(),
            fixed_ram: shape.fixed_ram,
            #[cfg(feature = "secure")]
            secure: None,
            cfg,
        }
    }

    /// The most RAM a link with this config holds, in bytes: its fixed
    /// buffers (the send ring, the transmit window, the reorder buffer, the
    /// datagram slots, the frame scratch) plus the inbox's worst case (the
    /// receive budget, its event queue, and a reassembly buffer of
    /// `max_message` per reliable channel). A function of the config alone;
    /// [`ram_bytes`](Self::ram_bytes) never exceeds it.
    pub fn ram_bound(cfg: &LinkConfig) -> usize {
        let shape = Shape::of::<A>(cfg);
        shape.fixed_ram
            + shape.scratch
            + Inbox::ram_bound(
                cfg.rx_budget,
                cfg.max_message,
                cfg.reliable_channels.count_ones() as usize,
            )
    }

    /// RAM this link holds right now (see [`ram_bound`](Self::ram_bound)).
    pub fn ram_bytes(&self) -> usize {
        self.fixed_ram + self.scratch_bytes() + self.inbox.ram_bytes()
    }

    // ---- Application side ------------------------------------------------

    /// Queue a message. Reliable channels take up to `max_message` bytes and
    /// fragment; best-effort channels take one frame's worth.
    pub fn send(&mut self, channel: u8, payload: &[u8]) -> Result<(), SendError> {
        if channel >= 8 {
            return Err(SendError::BadChannel);
        }
        if self.cfg.is_reliable(channel) {
            if payload.len() > self.cfg.max_message || payload.len() > self.cfg.send_budget {
                return Err(SendError::TooBig);
            }
            if self.pending.live_bytes() + self.tx.bytes() + payload.len() > self.cfg.send_budget
                || self.pending.push(channel, payload).is_err()
            {
                return Err(SendError::Full);
            }
        } else {
            if payload.len() > self.cfg.max_payload as usize {
                return Err(SendError::TooBig);
            }
            let queued = self.datagrams.push_with(channel, |slot| {
                slot[..payload.len()].copy_from_slice(payload);
                Some(payload.len())
            });
            if !queued {
                self.counters.datagrams_dropped += 1;
                return Err(SendError::Full);
            }
        }
        Ok(())
    }

    /// Queue a reliable message of `len` bytes that the caller keeps: an
    /// **external** message. Its bytes are read from the caller's buffer, a
    /// fragment at a time, by [`poll_transmit_with`](Self::poll_transmit_with)
    /// and copied into the transmit window (which keeps them for resends);
    /// nothing is copied into the send ring, and it does not count against
    /// `send_budget`. The caller keeps the buffer unchanged while
    /// [`external_in_flight`](Self::external_in_flight) is true.
    ///
    /// At most one at a time (`Full` while one is in flight). It keeps its
    /// place in its channel's order, and a lower channel still overtakes it
    /// at a frame boundary. A reset drops it like any queued message.
    pub fn send_external(&mut self, channel: u8, len: usize) -> Result<(), SendError> {
        if channel >= 8 || !self.cfg.is_reliable(channel) {
            return Err(SendError::BadChannel);
        }
        if len > self.cfg.max_message {
            return Err(SendError::TooBig);
        }
        self.pending
            .push_external(channel, len)
            .map_err(|_| SendError::Full)
    }

    /// An external message has bytes not yet cut into frames: its buffer is
    /// still the link's to read. Once false the caller may reuse it; the
    /// frames already cut are in the transmit window.
    pub fn external_in_flight(&self) -> bool {
        self.pending.external_untaken()
    }

    /// Withdraw the external message before any of it has been sent.
    /// [`ExternalStarted`] once its first fragment has been cut: the peer may
    /// hold part of it, so it can only be finished (wait for
    /// [`external_in_flight`](Self::external_in_flight)) or abandoned with the
    /// session ([`restart`](Self::restart)). `Ok` when none is queued.
    pub fn cancel_external(&mut self) -> Result<(), ExternalStarted> {
        self.pending.cancel_external().map_err(|()| ExternalStarted)
    }

    /// Move log records from `ring` into the log channel while there is room.
    /// Records stay in the ring (which drops its oldest) while the link is
    /// down, so logging stays cheap and bounded then.
    pub fn pump_log<const N: usize>(&mut self, now: Micros, ring: &mut LogRing<N>, channel: u8) {
        if self.state != LinkState::Established || self.is_stalled(now) {
            return;
        }
        let limit = self.tx_payload;
        while self
            .datagrams
            .push_with(channel, |slot| ring.pop_into(&mut slot[..limit]))
        {}
    }

    /// The next event: a message, text, link up, or reset.
    pub fn recv(&mut self) -> Option<LinkEvent> {
        let ev = self.inbox.pop()?;
        let drained = self.arq.drain(&mut self.inbox);
        self.counters.oversize_messages = self.inbox.oversize_messages();
        match drained {
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
        if !bytes.is_empty() {
            self.heard_while_connecting(now);
        }
        for &b in bytes {
            match self.deframer.push(now, b) {
                Deframed::Nothing => {}
                Deframed::Text => self.flush_text(),
                Deframed::Overflow => self.counters.oversize_frames += 1,
                Deframed::Abandoned => self.counters.stale_partials += 1,
                Deframed::Frame => {
                    let mut raw = mem::take(&mut self.rx_raw);
                    raw.clear();
                    let body = self.deframer.frame();
                    let decoded = if self.cfg.escape_ff {
                        frame::unwrap_stream(body, &mut raw)
                    } else {
                        frame::unwrap_stream_plain(body, &mut raw)
                    };
                    let ok = match decoded {
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
        self.heard_while_connecting(now);
        self.on_frame(now, frame);
    }

    /// The next frame to write, if any. Call until `None` whenever the
    /// transport can take more. Each returned slice is one whole frame (write
    /// it as one datagram, or as bytes on a stream).
    ///
    /// While an external message is queued ([`send_external`](Self::send_external))
    /// call [`poll_transmit_with`](Self::poll_transmit_with) instead: without a
    /// source its channel waits (lower channels still go).
    pub fn poll_transmit(&mut self, now: Micros) -> Option<&[u8]> {
        self.poll_transmit_inner(now, None)
    }

    /// [`poll_transmit`](Self::poll_transmit), with `source(offset, out)`
    /// filling `out` from the external message's bytes at `offset` whenever a
    /// fragment of it is cut.
    pub fn poll_transmit_with(
        &mut self,
        now: Micros,
        source: &mut dyn FnMut(usize, &mut [u8]),
    ) -> Option<&[u8]> {
        self.poll_transmit_inner(now, Some(source))
    }

    fn poll_transmit_inner(
        &mut self,
        now: Micros,
        mut source: Option<&mut dyn FnMut(usize, &mut [u8])>,
    ) -> Option<&[u8]> {
        self.service_timers(now);
        let mut sent = self.pick_and_emit(now, &mut source);
        if !sent && self.state == LinkState::Connecting {
            // A reset inside pick_and_emit leaves a SYN due now.
            sent = self.pick_and_emit(now, &mut source);
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
        let mut t = self
            .deframer
            .idle_deadline(self.cfg.idle_flush, self.cfg.frame_abandon);
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
                    min(self.probe_deadline());
                }
                min(Some(self.last_tx + self.cfg.keepalive));
            }
        }
        #[cfg(feature = "secure")]
        min(self.secure_deadline());
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

    /// The nonce the peer opened its session with, once a SYN has been heard
    /// (a peer that restarts draws a new one, which is what
    /// [`ResetReason::PeerRestarted`] means). For diagnostics: a desk tool
    /// prints it so two boots of one board can be told apart.
    pub fn peer_nonce(&self) -> Option<u32> {
        self.peer_nonce
    }

    pub fn counters(&self) -> &LinkCounters {
        &self.counters
    }

    /// The most recent round trip this end measured for a data frame (sent
    /// to acknowledged), with the running sample count — a host polls it to
    /// read the link-level RTT distribution (`lp-cli link rtt`).
    pub fn rtt_last_sample(&self) -> (Micros, u32) {
        self.rtt.last_sample()
    }

    /// The smoothed round-trip time this end's resend timer works from (the
    /// initial timeout until the first sample).
    pub fn srtt(&self) -> Micros {
        self.rtt.srtt()
    }

    pub fn config(&self) -> &LinkConfig {
        &self.cfg
    }

    /// How long since anything verified arrived.
    pub fn peer_silent_for(&self, now: Micros) -> Micros {
        now.saturating_sub(self.last_rx)
    }

    /// Up, but nothing heard for `stall_after`: the other end is not there
    /// right now. Not a reset (it may come back); a signal for the edge and
    /// for what is worth sending.
    pub fn is_stalled(&self, now: Micros) -> bool {
        self.state == LinkState::Established && self.peer_silent_for(now) >= self.cfg.stall_after
    }

    /// Best-effort messages [`send`](Self::send) would queue right now (free
    /// datagram slots). A caller that takes each message out of its own
    /// buffer (a firmware log ring) asks first, so a message the link would
    /// refuse stays where it was instead of being taken and lost.
    pub fn datagram_room(&self) -> usize {
        self.datagrams.free_slots()
    }

    /// Nothing queued, nothing unacknowledged.
    pub fn is_idle(&self) -> bool {
        self.pending.is_empty() && self.tx.is_empty() && self.datagrams.is_empty()
    }

    /// Payload bytes held right now: queued to send, unacknowledged, held out
    /// of order, reassembling, and waiting for `recv()`.
    pub fn buffered_bytes(&self) -> usize {
        self.pending.live_bytes()
            + self.tx.bytes()
            + self.datagrams.bytes()
            + self.arq.reorder_bytes()
            + self.inbox.bytes()
    }

    /// Payload bytes the reliability machinery holds: sent but unacknowledged,
    /// plus received out of order. What the windows cost in RAM.
    pub fn window_bytes(&self) -> usize {
        self.tx.bytes() + self.arq.reorder_bytes()
    }

    /// Frame scratch capacity (encode, decode and deframing buffers), reserved
    /// in `new`.
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
            #[cfg(feature = "secure")]
            if let Some(verified) = self.on_secure_aware_syn(now, raw) {
                return verified;
            }
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
        #[cfg(feature = "secure")]
        let max_body = self.cfg.max_payload as usize + self.seal_overhead();
        #[cfg(not(feature = "secure"))]
        let max_body = self.cfg.max_payload as usize;
        if raw.len() > HEADER_LEN + max_body + crc.len() {
            // Longer than any frame we agreed to take (a datagram transport
            // hands frames over whole, unchecked).
            self.counters.oversize_frames += 1;
            return false;
        }
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
        #[cfg(feature = "secure")]
        if self.secure.is_some() {
            return self.on_sealed_frame(now, &hdr, raw, body);
        }
        self.on_verified(now, &hdr, body)
    }

    /// A frame that verified under the session key (and, on a secure link,
    /// opened): its plaintext body.
    fn on_verified(&mut self, now: Micros, hdr: &Header, body: &[u8]) -> bool {
        self.counters.frames_rx += 1;
        self.last_rx = now;
        if self.state == LinkState::Connecting {
            // Only a peer that knows our current nonce can key a frame so.
            self.establish(now);
        }
        match hdr.kind {
            FrameKind::Data => {
                if self.on_ack_fields(now, hdr, 0, None) {
                    self.on_data(now, hdr, body);
                }
            }
            FrameKind::Datagram => {
                if !self.on_ack_fields(now, hdr, 0, None) {
                    return true;
                }
                // Best effort, but never silent: datagrams carry their own
                // 8-bit sequence, so a gap is counted and a duplicate dropped.
                let next = self.dgram_rx_next.unwrap_or(hdr.seq);
                let ahead = seq_dist(next, hdr.seq);
                // Only a short way back is a duplicate; anything else is a
                // gap going forward (a long outage must not read as "old").
                if ahead >= 224 {
                    self.counters.duplicates += 1;
                    return true;
                }
                self.counters.datagrams_lost += ahead as u32;
                self.dgram_rx_next = Some(hdr.seq.wrapping_add(1));
                if self.inbox.has_room(hdr.chan, body.len()) {
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
                self.on_ack_fields(now, hdr, sack, hdr.fin.then_some(hdr.seq));
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
        let verdict = self.arq.on_data(hdr.seq, frag, &mut self.inbox);
        self.counters.oversize_messages = self.inbox.oversize_messages();
        match verdict {
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

    /// Apply a frame's ACK and window; `false` if it ended the session.
    fn on_ack_fields(&mut self, now: Micros, hdr: &Header, sack: u32, trigger: Option<u8>) -> bool {
        if !A::RELIABLE {
            return true;
        }
        let Some(acked) = self.tx.ack_to(hdr.ack, now) else {
            // Past the last frame we sent: the peer took a frame we never sent
            // (one that passed the checksum without being ours) and now waits
            // beyond it, so every later ACK would be ignored here and the
            // flight resent until the retry limit. The session is already
            // wrong; end it now. Further off (a stale ACK from behind `base`,
            // or more than any receive window ahead) is ignored, as before.
            if (1..=A::MAX_WINDOW).contains(&seq_dist(self.tx.next_seq(), hdr.ack)) {
                self.protocol_error();
                return false;
            }
            return true;
        };
        if acked.frames > 0 {
            self.backoff = 0;
            self.probe_armed = true;
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
            reorder_threshold: self.cfg.reorder_threshold,
        };
        self.counters.fast_retransmits += A::on_feedback(&mut self.tx, fb) as u32;
        true
    }

    fn flush_text(&mut self) {
        let text = self.deframer.text();
        if self.inbox.push_text(text) {
            self.counters.text_bytes += text.len() as u32;
        } else {
            self.counters.text_dropped += text.len() as u32;
        }
        self.deframer.clear_text();
    }

    // ---- Transmit path ---------------------------------------------------

    fn service_timers(&mut self, now: Micros) {
        #[cfg(feature = "secure")]
        self.secure_timers(now);
        if self
            .deframer
            .idle_deadline(self.cfg.idle_flush, self.cfg.frame_abandon)
            .is_some_and(|t| t <= now)
        {
            match self.deframer.flush_idle() {
                IdleFlush::Text => self.flush_text(),
                IdleFlush::Garbage => self.counters.stale_partials += 1,
                IdleFlush::Nothing => {}
            }
        }
        if self.state == LinkState::Established
            && A::RELIABLE
            && self.probe_deadline().is_some_and(|t| t <= now)
        {
            self.fire_probe();
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
    fn pick_and_emit(
        &mut self,
        now: Micros,
        source: &mut Option<&mut dyn FnMut(usize, &mut [u8])>,
    ) -> bool {
        if self.state == LinkState::Connecting {
            if self.syn_due.is_some_and(|t| t <= now) {
                self.syn_due = Some(now + (self.cfg.syn_interval << self.syn_doublings));
                if self.syn_doublings < self.cfg.syn_backoff {
                    self.syn_doublings += 1;
                }
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
        let every = self.cfg.datagram_every;
        if every != 0 && self.data_run >= every && self.emit_datagram() {
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
            && let Some(seq) = self.next_fragment(source)
        {
            self.emit_data(now, seq);
            return true;
        }
        if self.emit_datagram() {
            return true;
        }
        if ack_ready || now >= self.last_tx + self.cfg.keepalive {
            self.emit_ack();
            return true;
        }
        false
    }

    /// When the tail-loss probe fires: two smoothed round trips (plus the
    /// peer's ACK delay) after the last data frame, if the flight is still
    /// unacknowledged, nothing is waiting to be resent, and that comes before
    /// the retransmit timer. The probe resends the newest frame, so the ACK it
    /// draws reveals any hole behind it (TCP's TLP).
    fn probe_deadline(&self) -> Option<Micros> {
        if !self.probe_armed || self.tx.first_unsent().is_some() {
            return None;
        }
        self.tx
            .iter()
            .rev()
            .find(|e| !e.sacked && e.sent_at.is_some())?;
        let pto = (2 * self.rtt.srtt() + self.cfg.ack_delay).max(self.cfg.min_rto / 2);
        let t = self.last_data_tx + pto;
        (self
            .tx
            .next_timer(self.rto())
            .is_none_or(|rto_at| t < rto_at))
        .then_some(t)
    }

    fn fire_probe(&mut self) {
        self.probe_armed = false;
        if let Some(e) = self
            .tx
            .iter_mut()
            .rev()
            .find(|e| !e.sacked && e.sent_at.is_some())
        {
            e.sent_at = None;
            self.counters.probes += 1;
        }
    }

    fn can_send_new(&self) -> bool {
        if !A::RELIABLE {
            return true;
        }
        !self.tx.is_full()
            && (seq_dist(self.tx.base(), self.tx.next_seq()) as usize) < self.peer_win as usize
    }

    /// Cut the next fragment of a queued message into the transmit window;
    /// its sequence number.
    fn next_fragment(
        &mut self,
        source: &mut Option<&mut dyn FnMut(usize, &mut [u8])>,
    ) -> Option<u8> {
        let (pending, n) = (&mut self.pending, self.tx_payload);
        self.tx.push_with(|slot| {
            let source = source
                .as_mut()
                .map(|f| &mut **f as &mut dyn FnMut(usize, &mut [u8]));
            let t = pending.take(&mut slot[..n], source)?;
            Some((t.chan, t.first, t.fin, t.len))
        })
    }

    /// Send the oldest queued datagram, if any.
    fn emit_datagram(&mut self) -> bool {
        while let Some((chan, data)) = self.datagrams.front() {
            if data.len() > self.tx_payload {
                // Queued before the peer's smaller frame size was known.
                self.datagrams.pop_front();
                self.counters.datagrams_dropped += 1;
                continue;
            }
            let mut hdr = self.header(FrameKind::Datagram, chan);
            hdr.seq = self.dgram_tx_seq;
            self.dgram_tx_seq = self.dgram_tx_seq.wrapping_add(1);
            let key = self.key();
            let Some((_, data)) = self.datagrams.front() else {
                return false;
            };
            frame::encode_raw(self.cfg.crc, key, &hdr, data, &mut self.raw);
            #[cfg(feature = "secure")]
            self.seal_raw();
            finish(
                self.cfg.framing,
                self.cfg.escape_ff,
                &mut self.raw,
                &mut self.out,
            );
            self.datagrams.pop_front();
            self.data_run = 0;
            return true;
        }
        false
    }

    fn emit_data(&mut self, now: Micros, seq: u8) {
        let mut hdr = self.header(FrameKind::Data, 0);
        self.send_order = self.send_order.wrapping_add(1);
        let key = self.key();
        let Some((e, payload)) = self.tx.frame_mut(seq) else {
            return;
        };
        e.sent_at = Some(now);
        e.sent_order = self.send_order;
        e.sends = e.sends.saturating_add(1);
        if e.sends > 1 {
            self.counters.retransmits += 1;
        } else {
            self.probe_armed = true;
        }
        self.last_data_tx = now;
        self.counters.data_frames_tx += 1;
        self.data_run = self.data_run.saturating_add(1);
        hdr.chan = e.chan;
        hdr.first = e.first;
        hdr.fin = e.fin;
        hdr.seq = seq;
        frame::encode_raw(self.cfg.crc, key, &hdr, payload, &mut self.raw);
        #[cfg(feature = "secure")]
        self.seal_raw();
        finish(
            self.cfg.framing,
            self.cfg.escape_ff,
            &mut self.raw,
            &mut self.out,
        );
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
        #[cfg(feature = "secure")]
        if self.secure.is_some() {
            self.emit_secure_syn();
            return;
        }
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
        finish(
            self.cfg.framing,
            self.cfg.escape_ff,
            &mut self.raw,
            &mut self.out,
        );
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
        #[cfg(feature = "secure")]
        self.seal_raw();
        finish(
            self.cfg.framing,
            self.cfg.escape_ff,
            &mut self.raw,
            &mut self.out,
        );
    }

    /// Frames past our ACK we can take now: room left in the receive budget
    /// once what waits for the application is counted. A frame may complete
    /// a message, which the inbox charges [`EVENT_COST`] on top of its bytes.
    ///
    /// Partial messages do not close the window: they drain only as more
    /// frames arrive, so counting them could leave two channels mid-message
    /// and nothing able to move (the fuzzer found it). Each channel's own
    /// room check counts its partial instead.
    fn adv_window(&self) -> u8 {
        let used = self.inbox.ready_bytes() + self.arq.reorder_bytes();
        let per_frame = self.cfg.max_payload as usize + EVENT_COST;
        let free = self.inbox.budget().saturating_sub(used) / per_frame;
        free.min(self.cfg.rx_window.min(A::MAX_WINDOW) as usize) as u8
    }

    fn key(&self) -> u32 {
        self.nonce ^ self.peer_nonce.unwrap_or(0)
    }

    // ---- Lifecycle -------------------------------------------------------

    /// Something arrived while handshaking: somebody may be listening, so a
    /// backed-off SYN gap (`syn_backoff`) goes back to `syn_interval`.
    fn heard_while_connecting(&mut self, now: Micros) {
        if self.state != LinkState::Connecting || self.syn_doublings == 0 {
            return;
        }
        self.syn_doublings = 0;
        let soon = now + self.cfg.syn_interval;
        if self.syn_due.is_none_or(|t| t > soon) {
            self.syn_due = Some(soon);
        }
    }

    fn establish(&mut self, now: Micros) {
        self.state = LinkState::Established;
        self.tx_payload = self.cfg.max_payload.min(self.peer_max_payload).max(1) as usize;
        self.peer_win = self.peer_rx_window;
        self.last_rx = now;
        self.last_tx = now;
        self.counters.ups += 1;
        self.inbox.push_lifecycle(LinkEvent::Up {
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
        self.datagrams.clear();
        self.data_run = 0;
        self.dgram_tx_seq = 0;
        self.dgram_rx_next = None;
        self.inbox.abort_all();
        self.backoff = 0;
        self.peer_win = 0;
        self.ack_due = None;
        self.ack_explicit = false;
        self.ack_trigger = None;
        self.unacked_rx = 0;
        self.syn_owed = false;
        self.probe_armed = false;
        self.syn_due = Some(now);
        self.syn_doublings = 0;
        self.counters.resets += 1;
        self.inbox.push_lifecycle(LinkEvent::Reset {
            reason,
            generation: self.generation,
        });
        #[cfg(feature = "secure")]
        self.secure_reset();
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
fn finish(framing: Framing, escape_ff: bool, raw: &mut Vec<u8>, out: &mut Vec<u8>) {
    match framing {
        Framing::Stream if escape_ff => frame::wrap_stream(raw, out),
        Framing::Stream => frame::wrap_stream_plain(raw, out),
        // header ‖ body ‖ crc is exactly one datagram.
        Framing::Datagram => mem::swap(raw, out),
    }
}

/// What a config makes a link allocate in `new`.
struct Shape {
    /// Frames in the transmit window.
    tx_window: usize,
    /// Largest COBS decode (a damaged body can decode longer than a frame).
    max_rx_raw: usize,
    /// Largest COBS-FF body between the delimiters.
    max_cobs: usize,
    /// Largest frame as written to the transport.
    max_wire: usize,
    /// Frame scratch: `raw`, `out`, `rx_raw` and the deframer's buffers.
    scratch: usize,
    /// Everything else allocated once: the send ring, the transmit window, the
    /// reorder buffer, the datagram slots, and the link itself.
    fixed_ram: usize,
}

impl Shape {
    fn of<A: Arq>(cfg: &LinkConfig) -> Self {
        let max_payload = cfg.max_payload as usize;
        let tx_window = cfg.tx_window.min(A::MAX_WINDOW).max(1) as usize;
        // Largest decoded frame: header, body, checksum.
        let max_raw = HEADER_LEN + max_payload.max(SYN_LEN) + cfg.crc.len();
        let max_cobs = cobs::max_encoded_no_ff_len(max_raw);
        let (max_wire, max_rx_raw, deframer) = match cfg.framing {
            Framing::Stream => (max_cobs + 2, max_cobs, Deframer::ram_bound(max_cobs)),
            Framing::Datagram => (max_raw, 0, 0),
        };
        Shape {
            tx_window,
            max_rx_raw,
            max_cobs,
            max_wire,
            scratch: max_rx_raw + 2 * max_wire + deframer,
            fixed_ram: size_of::<Link<A>>()
                + SendQueue::ram_bound(cfg.send_budget, cfg.send_queue)
                + TxQueue::ram_bound(tx_window, max_payload)
                + A::ram_bound(cfg.rx_window.min(A::MAX_WINDOW), max_payload)
                + DatagramQueue::ram_bound(cfg.datagram_queue, max_payload),
        }
    }
}

/// The nonce after a restart: a bijective mix, never 0 ("unknown" on the wire).
fn next_nonce(n: u32) -> u32 {
    let mut z = n.wrapping_add(0x9E37_79B9);
    z = (z ^ (z >> 16)).wrapping_mul(0x85EB_CA6B);
    z = (z ^ (z >> 13)).wrapping_mul(0xC2B2_AE35);
    (z ^ (z >> 16)).max(1)
}
