//! A secure link's handshake, inside lp-link's own SYN (feature `secure`).
//!
//! The SYN keeps its 12 bytes and grows an extension (`frame::secure_syn`):
//! the initiator's every SYN carries Noise's msg1 until it is up; the
//! responder answers with msg2 once its edge has found the key. Plain SYNs
//! are untouched, so a plain link's bytes do not move.
//!
//! ```text
//! initiator                                   responder (the device)
//!   SYN + msg1(key id, e_i)  ───────────►  KeyLookup{key id} → edge
//!                                         ◄── provide_keys(candidates) / refuse
//!                            ◄───────────  SYN + msg2(e_r, enc(nonce))   [half-open]
//!   Up; sealed ACK at once   ───────────►  first frame that opens → Up
//! ```
//!
//! - **Initiator.** One ephemeral per session, so every resent msg1 is the
//!   same bytes. Up on a msg2 that answers its nonce and whose encrypted
//!   payload repeats the SYN's nonce. A refusal stops its SYNs (the edge
//!   picks `retry_with` or `restart`); a responder that is not secure raises
//!   `PeerNotSecure` and never comes up (no downgrade). A presence SYN with a
//!   new nonce is a rebooted device, and resets it, as on a plain link.
//! - **Responder.** It parks msg1 and asks its edge (`KeyLookup`, 2 s before
//!   it refuses `Busy`). It tries each candidate PSK (no DH), writes msg2 for
//!   the first that matches, and goes half-open: keys split, still
//!   `Connecting`, msg2 resent on every SYN. The initiator's first frame that
//!   opens under the session key is the key confirmation, and only then is
//!   the responder up. An established responder resets for a new initiator
//!   nonce **only once that msg1 verifies**; a plain SYN, or a msg1 that
//!   fails, never knocks a secure session down.
//! - **Every reset** wipes the session's keys and handshake; the next
//!   session is a fresh handshake with fresh ephemerals on both ends.

use alloc::boxed::Box;
use alloc::vec::Vec;

use zeroize::Zeroize;

use super::secure_state::{
    HalfOpen, LOOKUP_TIMEOUT, PendingLookup, Refusal, SecureState, SessionKeys, ephemeral_of,
};
use super::{Link, LinkState, Shape, finish};
use crate::Micros;
use crate::arq::Arq;
use crate::cobs;
use crate::deframer::Deframer;
use crate::frame::secure_syn::{SECURE_SYN_MAX_LEN, SecureSyn, SynExt};
use crate::frame::{self, FrameKind, HEADER_LEN, Header, SynBody};
use crate::inbox::Inbox;
use crate::link_config::{Framing, LinkConfig, SEAL_OVERHEAD};
use crate::link_event::ResetReason;
use crate::secure_channel::cipher_state::CipherKey;
use crate::secure_channel::{
    Initiator, KeyId, MSG2_LEN, MSG2_PAYLOAD_LEN, Psk, RefusalReason, ReplayWindow, Responder,
    SecureEvent, SecureRole, SessionAuth, prologue,
};

impl<A: Arq> Link<A> {
    /// A secure link (see the module docs). `entropy` fills a buffer with
    /// fresh random bytes (32 per handshake on each end); `nonce` as for
    /// [`Link::new`]. An initiator starts its handshake at once.
    pub fn new_secure(
        cfg: LinkConfig,
        nonce: u32,
        role: SecureRole,
        entropy: fn(&mut [u8]),
    ) -> Self {
        let mut link = Self::new(cfg, nonce);
        // A plain link's buffers, re-cut once for sealed frames and the
        // 76-byte msg1 SYN (so `Link::new` itself is untouched).
        let shape = Shape::secure::<A>(&link.cfg);
        let stream = link.cfg.framing == Framing::Stream;
        link.deframer =
            Deframer::new(shape.max_cobs, stream).with_text_mark(link.cfg.escape_ff && stream);
        link.rx_raw = Vec::with_capacity(shape.max_rx_raw);
        link.raw = Vec::with_capacity(shape.max_wire);
        link.out = Vec::with_capacity(shape.max_wire);
        link.fixed_ram = shape.fixed_ram;
        link.secure = Some(Box::new(SecureState::new(role, entropy)));
        link.secure_reset();
        link
    }

    /// [`ram_bound`](Self::ram_bound) for a link built with
    /// [`new_secure`](Self::new_secure): larger frame scratch (sealed frames
    /// and the 76-byte msg1 SYN) and the secure state.
    pub fn ram_bound_secure(cfg: &LinkConfig) -> usize {
        let shape = Shape::secure::<A>(cfg);
        shape.fixed_ram
            + shape.scratch
            + Inbox::ram_bound(
                cfg.rx_budget,
                cfg.max_message,
                cfg.reliable_channels.count_ones() as usize,
            )
    }

    /// Built with [`new_secure`](Self::new_secure).
    pub fn is_secure(&self) -> bool {
        self.secure.is_some()
    }

    /// The next handshake event for the edge. Drain it after every input
    /// (`on_bytes`/`on_datagram`) and answer each `KeyLookup` with
    /// [`provide_keys`](Self::provide_keys) or [`refuse`](Self::refuse),
    /// then `poll_transmit`.
    pub fn poll_secure_event(&mut self) -> Option<SecureEvent> {
        self.secure.as_mut()?.events.pop_front()
    }

    /// Responder, once up: the key this session authenticated and which
    /// candidate (an index into what the edge gave `provide_keys`) matched.
    pub fn session_auth(&self) -> Option<SessionAuth> {
        if self.state != LinkState::Established {
            return None;
        }
        self.secure.as_ref()?.auth
    }

    /// Responder: answer a `KeyLookup` for `key_id` with the PSKs of every
    /// entry with that id, the best tier first. They are tried in order; the
    /// first that verifies msg1 wins. None verify: the initiator is refused
    /// `WrongKey` and a [`SecureEvent::WrongKey`] follows. An answer for a
    /// key id no longer pending is ignored. Call `poll_transmit` after.
    pub fn provide_keys(&mut self, key_id: KeyId, candidates: &[Psk]) {
        let Some(p) = self.take_pending(key_id) else {
            return;
        };
        let responder = Responder::new(&prologue(&p.key_id, p.nonce));
        let matched = candidates
            .iter()
            .take(usize::from(u8::MAX))
            .enumerate()
            .find_map(|(i, psk)| responder.read_msg1(&p.msg, psk).ok().map(|r| (i, r)));
        let Some((candidate, ready)) = matched else {
            self.refuse_pending(&p, RefusalReason::WrongKey, 0);
            if let Some(sec) = self.secure.as_mut() {
                sec.push_event(SecureEvent::WrongKey { key_id });
            }
            return;
        };
        let now = self.last_rx;
        if self.state == LinkState::Established {
            // Only now, with msg1 verified, may the old session go (a plain
            // SYN or a msg1 that fails never resets a secure responder).
            self.reset(now, ResetReason::PeerRestarted);
        }
        let Some(sec) = self.secure.as_mut() else {
            return;
        };
        let mut e = sec.fresh_secret();
        let mut msg2 = [0u8; MSG2_LEN];
        let keys = ready.write_msg2(e, &self.nonce.to_le_bytes(), &mut msg2);
        e.zeroize();
        let Ok(keys) = keys else {
            return;
        };
        sec.keys = Some(SessionKeys {
            send: CipherKey::new(keys.send),
            recv: CipherKey::new(keys.recv),
        });
        sec.send_ctr = 0;
        sec.replay = fresh_replay::<A>();
        sec.half_open = Some(HalfOpen {
            peer_nonce: p.nonce,
            e_i: p.e_i(),
            msg2,
            auth: SessionAuth {
                key_id,
                candidate: candidate as u8,
            },
        });
        sec.last_refusal = None;
        self.peer_nonce = Some(p.nonce);
        self.peer_max_payload = p.max_payload;
        self.peer_rx_window = p.rx_window;
        self.counters.handshakes += 1;
        self.syn_due = Some(now);
    }

    /// Responder: refuse the pending `KeyLookup` for `key_id`: `UnknownKey`
    /// (no entry has it: not a strike against the device's backoff),
    /// `Backoff` (with `retry_after_ms`), or `Busy`. Call `poll_transmit`
    /// after.
    pub fn refuse(&mut self, key_id: KeyId, reason: RefusalReason, retry_after_ms: u32) {
        if let Some(p) = self.take_pending(key_id) {
            self.refuse_pending(&p, reason, retry_after_ms);
        }
    }

    /// Initiator: after a refusal, try another key (the same session nonce,
    /// a fresh ephemeral). SYNs start again at once.
    pub fn retry_with(&mut self, key_id: KeyId, psk: Psk) {
        let Some(sec) = self.secure.as_mut() else {
            return;
        };
        if !sec.is_initiator() {
            return;
        }
        sec.role = SecureRole::Initiator { key_id, psk };
        sec.refused = false;
        self.start_initiator();
        if self.state == LinkState::Connecting {
            self.syn_due = Some(0);
        }
    }

    /// Tests only: this session's `(send, recv)` keys, once split; `None`
    /// before the handshake and after a reset wiped them.
    #[cfg(feature = "sim")]
    #[doc(hidden)]
    pub fn secure_keys_for_test(&self) -> Option<([u8; 32], [u8; 32])> {
        let keys = self.secure.as_ref()?.keys.as_ref()?;
        Some((*keys.send.bytes(), *keys.recv.bytes()))
    }

    /// Tests only: move this session's send counter (the restart before the
    /// counter space runs out).
    #[cfg(feature = "sim")]
    #[doc(hidden)]
    pub fn set_send_counter_for_test(&mut self, ctr: u32) {
        if let Some(sec) = self.secure.as_mut() {
            sec.send_ctr = ctr;
        }
    }

    /// Tests only: a responder between msg2 and the initiator's first sealed
    /// frame.
    #[cfg(feature = "sim")]
    #[doc(hidden)]
    pub fn is_half_open_for_test(&self) -> bool {
        self.secure.as_ref().is_some_and(|s| s.half_open.is_some())
    }

    // ---- Hooks from the plain link -----------------------------------------

    /// A SYN, on a link built with the feature on: `Some(verified)` when it is
    /// this module's to handle (any SYN on a secure link; a secure SYN heard
    /// by a plain link), `None` to leave it to the plain path.
    pub(super) fn on_secure_aware_syn(&mut self, now: Micros, raw: &[u8]) -> Option<bool> {
        let body = frame::verify(self.cfg.crc, 0, raw)?;
        let syn = SecureSyn::parse(body);
        if self.secure.is_none() {
            // A plain link and a secure peer will never come up: say so.
            if !syn.is_some_and(|s| s.secure) {
                return None;
            }
            self.counters.frames_rx += 1;
            self.last_rx = now;
            self.counters.secure_required += 1;
            return Some(true);
        }
        let Some(syn) = syn else {
            self.counters.bad_frames += 1;
            return Some(false);
        };
        self.counters.frames_rx += 1;
        self.last_rx = now;
        if self.secure.as_ref().is_some_and(|s| s.is_initiator()) {
            self.on_syn_as_initiator(now, syn);
        } else {
            self.on_syn_as_responder(now, syn);
        }
        Some(true)
    }

    /// Lookup deadline and counter exhaustion, run with the other timers.
    pub(super) fn secure_timers(&mut self, now: Micros) {
        let Some(sec) = self.secure.as_mut() else {
            return;
        };
        if sec.pending.as_ref().is_some_and(|p| p.deadline <= now)
            && let Some(p) = sec.pending.take()
        {
            self.refuse_pending(&p, RefusalReason::Busy, 0);
        }
        // The last counter is never used: a session that has sealed
        // u32::MAX frames starts a new one (~49 days at 1,000 frames/s).
        if self.state == LinkState::Established
            && self.secure.as_ref().is_some_and(|s| s.send_ctr == u32::MAX)
        {
            self.reset(now, ResetReason::Requested);
        }
    }

    /// When a pending lookup will be refused `Busy`.
    pub(super) fn secure_deadline(&self) -> Option<Micros> {
        Some(self.secure.as_ref()?.pending.as_ref()?.deadline)
    }

    /// Every reset: the session's keys and handshake are wiped; an initiator
    /// starts a fresh handshake under its new nonce.
    pub(super) fn secure_reset(&mut self) {
        let Some(sec) = self.secure.as_mut() else {
            return;
        };
        sec.clear_session();
        sec.replay = fresh_replay::<A>();
        if sec.is_initiator() {
            self.start_initiator();
        }
    }

    /// The SYN a secure link sends: msg1 (initiator), a refusal owed, msg2
    /// (half-open responder), or presence.
    pub(super) fn emit_secure_syn(&mut self) {
        let Some(sec) = self.secure.as_mut() else {
            return;
        };
        let peer = self.peer_nonce.unwrap_or(0);
        let (your, ext) = match &sec.role {
            SecureRole::Initiator { key_id, .. } => match &sec.initiator {
                Some(init) => (
                    peer,
                    SynExt::Msg1 {
                        key_id: key_id.0,
                        msg: *init.msg1(),
                    },
                ),
                None => (peer, SynExt::None),
            },
            SecureRole::Responder => {
                if let Some(r) = sec.refusal_owed.take() {
                    (
                        r.nonce,
                        SynExt::Refusal {
                            reason: r.reason.code(),
                            retry_after_ms: r.retry_after_ms,
                        },
                    )
                } else if let Some(h) = &sec.half_open {
                    (h.peer_nonce, SynExt::Msg2 { msg: h.msg2 })
                } else {
                    (peer, SynExt::None)
                }
            }
        };
        let syn = SecureSyn {
            base: SynBody {
                nonce: self.nonce,
                your,
                established: self.state == LinkState::Established,
                max_payload: self.cfg.max_payload,
                rx_window: self.adv_window(),
            },
            secure: true,
            ext,
        };
        let mut body = [0u8; SECURE_SYN_MAX_LEN];
        let n = syn.encode(&mut body);
        let hdr = Header {
            kind: FrameKind::Syn,
            fin: false,
            first: false,
            chan: 0,
            seq: 0,
            ack: 0,
            win: 0,
        };
        frame::encode_raw(self.cfg.crc, 0, &hdr, &body[..n], &mut self.raw);
        finish(
            self.cfg.framing,
            self.cfg.escape_ff,
            &mut self.raw,
            &mut self.out,
        );
    }

    // ---- Initiator --------------------------------------------------------

    fn on_syn_as_initiator(&mut self, now: Micros, syn: SecureSyn) {
        let Some(sec) = self.secure.as_mut() else {
            return;
        };
        if !syn.secure {
            if !sec.peer_not_secure_raised {
                sec.peer_not_secure_raised = true;
                sec.push_event(SecureEvent::PeerNotSecure);
            }
            return;
        }
        let base = syn.base;
        if self.state == LinkState::Established && Some(base.nonce) != self.peer_nonce {
            // The device restarted (or reset, or answered someone else's
            // msg1): a new session, as on a plain link. Our new msg1 goes at
            // once.
            self.reset(now, ResetReason::PeerRestarted);
            self.peer_nonce = Some(base.nonce);
            return;
        }
        match syn.ext {
            SynExt::Refusal {
                reason,
                retry_after_ms,
            } => {
                let ours = self.state == LinkState::Connecting && base.your == self.nonce;
                match RefusalReason::from_code(reason) {
                    Some(reason) if ours && !sec.refused => {
                        sec.refused = true;
                        sec.push_event(SecureEvent::Refused {
                            reason,
                            retry_after_ms,
                        });
                        self.syn_due = None;
                        self.counters.handshake_refusals += 1;
                    }
                    _ => self.counters.secure_syn_ignored += 1,
                }
            }
            SynExt::Msg2 { msg } => {
                if self.state == LinkState::Connecting && base.your == self.nonce {
                    let Some(init) = sec.initiator.as_ref() else {
                        self.counters.secure_syn_ignored += 1;
                        return;
                    };
                    let mut payload = [0u8; MSG2_PAYLOAD_LEN];
                    match init.read_msg2(&msg, &mut payload) {
                        Ok(keys) if u32::from_le_bytes(payload) == base.nonce => {
                            sec.initiator = None;
                            sec.keys = Some(SessionKeys {
                                send: CipherKey::new(keys.send),
                                recv: CipherKey::new(keys.recv),
                            });
                            sec.send_ctr = 0;
                            sec.replay = fresh_replay::<A>();
                            self.counters.handshakes += 1;
                            self.peer_nonce = Some(base.nonce);
                            self.peer_max_payload = base.max_payload;
                            self.peer_rx_window = base.rx_window;
                            self.establish(now);
                            // Key confirmation: a sealed frame at once, so
                            // the responder comes up within one flight.
                            self.ack_due = Some(now);
                        }
                        // Forged, damaged, or a nonce that does not match
                        // the one sealed inside: dropped, the handshake kept.
                        _ => self.counters.seal_failures += 1,
                    }
                } else if self.state == LinkState::Established && base.your == self.nonce {
                    // The responder is still half-open: our confirmation was
                    // lost. Another sealed frame is the answer (never a SYN).
                    self.ack_due = Some(now);
                } else if self.state == LinkState::Established {
                    // The responder is half-open for another initiator
                    // session (a replayed msg1 took its slot): ours is gone
                    // on its side. Start over; our new msg1 replaces it.
                    self.reset(now, ResetReason::PeerRestarted);
                } else {
                    self.counters.stale_syns += 1;
                }
            }
            SynExt::None => match self.state {
                LinkState::Established => {}
                LinkState::Connecting => {
                    self.peer_nonce = Some(base.nonce);
                    self.peer_max_payload = base.max_payload;
                    self.peer_rx_window = base.rx_window;
                    if !sec.refused {
                        self.syn_due = Some(now);
                    }
                }
            },
            SynExt::Msg1 { .. } => self.counters.secure_syn_ignored += 1,
        }
    }

    /// A fresh msg1 for the current nonce and key.
    fn start_initiator(&mut self) {
        let nonce = self.nonce;
        let Some(sec) = self.secure.as_mut() else {
            return;
        };
        let SecureRole::Initiator { key_id, psk } = &sec.role else {
            return;
        };
        let mut e = sec.fresh_secret();
        sec.initiator = Some(Initiator::new(&prologue(key_id, nonce), psk, e));
        e.zeroize();
    }

    // ---- Responder --------------------------------------------------------

    fn on_syn_as_responder(&mut self, now: Micros, syn: SecureSyn) {
        let Some(sec) = self.secure.as_mut() else {
            return;
        };
        let SynExt::Msg1 { key_id, msg } = syn.ext else {
            // A plain SYN (a plain host, or a forged one), or a responder's
            // SYN: never acted on, never a reset.
            self.counters.secure_syn_ignored += 1;
            return;
        };
        let nonce = syn.base.nonce;
        let e_i = ephemeral_of(&msg);
        if sec
            .half_open
            .as_ref()
            .is_some_and(|h| h.peer_nonce == nonce && h.e_i == e_i)
        {
            // msg2 was lost: the same msg2 again, now.
            self.syn_due = Some(now);
            return;
        }
        if self.state == LinkState::Established && Some(nonce) == self.peer_nonce {
            // A late resend of this session's msg1.
            self.counters.stale_syns += 1;
            return;
        }
        if sec
            .pending
            .as_ref()
            .is_some_and(|p| p.nonce == nonce && p.e_i() == e_i)
        {
            return; // Still looking it up.
        }
        if let Some((n, e, r)) = sec.last_refusal
            && n == nonce
            && e == e_i
        {
            // The refusal was lost: the same answer, without a second lookup
            // (or a second backoff charge).
            sec.refusal_owed = Some(r);
            self.owe_syn(now);
            return;
        }
        let key_id = KeyId(key_id);
        sec.pending = Some(PendingLookup {
            nonce,
            key_id,
            msg,
            max_payload: syn.base.max_payload,
            rx_window: syn.base.rx_window,
            deadline: now + LOOKUP_TIMEOUT,
        });
        sec.push_event(SecureEvent::KeyLookup { key_id });
    }

    fn take_pending(&mut self, key_id: KeyId) -> Option<PendingLookup> {
        let sec = self.secure.as_mut()?;
        if sec.pending.as_ref()?.key_id != key_id {
            return None;
        }
        sec.pending.take()
    }

    fn refuse_pending(&mut self, p: &PendingLookup, reason: RefusalReason, retry_after_ms: u32) {
        let Some(sec) = self.secure.as_mut() else {
            return;
        };
        let r = Refusal {
            nonce: p.nonce,
            reason,
            retry_after_ms,
        };
        sec.refusal_owed = Some(r);
        sec.last_refusal = Some((p.nonce, p.e_i(), r));
        self.counters.handshake_refusals += 1;
        let now = self.last_rx;
        self.owe_syn(now);
    }

    /// Send a SYN soon: on the SYN timer while connecting, as an owed SYN
    /// once up.
    fn owe_syn(&mut self, now: Micros) {
        match self.state {
            LinkState::Connecting => self.syn_due = Some(now),
            LinkState::Established => self.syn_owed = true,
        }
    }
}

impl Shape {
    /// A secure link's shape: `Shape::of`'s, with frames `SEAL_OVERHEAD`
    /// longer, SYN bodies up to 76 bytes, and the secure state (allocated
    /// once) in the fixed RAM.
    pub(super) fn secure<A: Arq>(cfg: &LinkConfig) -> Self {
        let plain = Self::of::<A>(cfg);
        let max_raw = HEADER_LEN
            + (cfg.max_payload as usize + SEAL_OVERHEAD).max(SECURE_SYN_MAX_LEN)
            + cfg.crc.len();
        let max_cobs = cobs::max_encoded_no_ff_len(max_raw);
        let (max_wire, max_rx_raw, deframer) = match cfg.framing {
            Framing::Stream => (max_cobs + 2, max_cobs, Deframer::ram_bound(max_cobs)),
            Framing::Datagram => (max_raw, 0, 0),
        };
        Shape {
            tx_window: plain.tx_window,
            max_rx_raw,
            max_cobs,
            max_wire,
            scratch: max_rx_raw + 2 * max_wire + deframer,
            fixed_ram: plain.fixed_ram + size_of::<SecureState>() + SecureState::heap_bytes(),
        }
    }
}

/// A fresh replay rule: a window on an ARQ link, strict on a no-ARQ one.
fn fresh_replay<A: Arq>() -> ReplayWindow {
    if A::RELIABLE {
        ReplayWindow::window()
    } else {
        ReplayWindow::strict()
    }
}
