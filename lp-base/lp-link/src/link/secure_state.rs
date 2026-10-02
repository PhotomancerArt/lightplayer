//! What a secure link keeps beside the plain link's state (feature
//! `secure`): its role and key, the handshake in progress, the session's
//! keys and counters, and the events waiting for the edge. Allocated once in
//! `Link::new_secure` and reset in place, so a session allocates nothing.

use alloc::collections::VecDeque;

use crate::Micros;
use crate::secure_channel::cipher_state::CipherKey;
use crate::secure_channel::{
    Initiator, KeyId, MSG1_LEN, MSG2_LEN, RefusalReason, ReplayWindow, SecureEvent, SecureRole,
    SessionAuth,
};

/// Events held for the edge at most; the oldest goes when a new one comes.
/// Each is raised at most once per SYN that causes it, and the edge drains
/// them after every input, so a handful is ample.
pub(super) const MAX_EVENTS: usize = 4;

/// How long a responder waits for its edge to answer a key lookup before it
/// refuses the initiator `Busy`.
pub(super) const LOOKUP_TIMEOUT: Micros = 2_000_000;

pub(crate) struct SecureState {
    pub(super) role: SecureRole,
    pub(super) entropy: fn(&mut [u8]),
    pub(super) events: VecDeque<SecureEvent>,

    // ---- The session (cleared, and its keys wiped, on every reset) -------
    /// After the handshake split: this end's send and receive keys.
    pub(super) keys: Option<SessionKeys>,
    /// The next counter this end seals a frame under.
    pub(super) send_ctr: u32,
    /// Which received counters are still acceptable.
    pub(super) replay: ReplayWindow,

    // ---- Initiator -------------------------------------------------------
    /// The handshake for this session's nonce: msg1 written, waiting for
    /// msg2. One ephemeral per session, so every resent msg1 is identical.
    pub(super) initiator: Option<Initiator>,
    /// The responder refused this key: SYNs stop until `retry_with` or a
    /// restart.
    pub(super) refused: bool,
    /// `PeerNotSecure` was raised this session.
    pub(super) peer_not_secure_raised: bool,

    // ---- Responder -------------------------------------------------------
    /// A msg1 waiting for the edge's answer to its key lookup.
    pub(super) pending: Option<PendingLookup>,
    /// msg2 written and the keys split, waiting for the initiator's first
    /// sealed frame (key confirmation).
    pub(super) half_open: Option<HalfOpen>,
    /// Once up: which key the session authenticated.
    pub(super) auth: Option<SessionAuth>,
    /// A refusal SYN to send once.
    pub(super) refusal_owed: Option<Refusal>,
    /// The last msg1 refused, so a resend of it gets the same answer without
    /// a second lookup (or a second backoff charge).
    pub(super) last_refusal: Option<(u32, [u8; 32], Refusal)>,
}

/// One direction's key each way.
pub(super) struct SessionKeys {
    pub(super) send: CipherKey,
    pub(super) recv: CipherKey,
}

/// A msg1 the responder heard and is looking a key up for.
pub(super) struct PendingLookup {
    /// The initiator's lp-link nonce (bound in the prologue).
    pub(super) nonce: u32,
    pub(super) key_id: KeyId,
    pub(super) msg: [u8; MSG1_LEN],
    /// The initiator's SYN fields, applied only if the msg1 verifies.
    pub(super) max_payload: u16,
    pub(super) rx_window: u8,
    /// Refused `Busy` when unanswered by then.
    pub(super) deadline: Micros,
}

impl PendingLookup {
    /// The initiator's ephemeral public key: with the nonce, what tells a
    /// resend of this msg1 from a new one.
    pub(super) fn e_i(&self) -> [u8; 32] {
        ephemeral_of(&self.msg)
    }
}

/// A responder session between msg2 and the first sealed frame.
pub(super) struct HalfOpen {
    pub(super) peer_nonce: u32,
    pub(super) e_i: [u8; 32],
    pub(super) msg2: [u8; MSG2_LEN],
    pub(super) auth: SessionAuth,
}

/// A refusal to send.
#[derive(Clone, Copy)]
pub(super) struct Refusal {
    /// The refused initiator's nonce (the SYN's `your`).
    pub(super) nonce: u32,
    pub(super) reason: RefusalReason,
    pub(super) retry_after_ms: u32,
}

impl SecureState {
    pub(super) fn new(role: SecureRole, entropy: fn(&mut [u8])) -> Self {
        SecureState {
            role,
            entropy,
            events: VecDeque::with_capacity(MAX_EVENTS),
            keys: None,
            send_ctr: 0,
            replay: ReplayWindow::window(),
            initiator: None,
            refused: false,
            peer_not_secure_raised: false,
            pending: None,
            half_open: None,
            auth: None,
            refusal_owed: None,
            last_refusal: None,
        }
    }

    pub(super) fn is_initiator(&self) -> bool {
        matches!(self.role, SecureRole::Initiator { .. })
    }

    /// Queue an event for the edge, dropping the oldest when full (no
    /// allocation: the queue's room was reserved in `new`).
    pub(super) fn push_event(&mut self, ev: SecureEvent) {
        if self.events.len() == MAX_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(ev);
    }

    /// 32 fresh bytes from the edge's entropy.
    pub(super) fn fresh_secret(&self) -> [u8; 32] {
        let mut b = [0u8; 32];
        (self.entropy)(&mut b);
        b
    }

    /// Everything of one session goes: keys (wiped as they drop), the
    /// handshake, the counters. The role, its key and queued events stay.
    pub(super) fn clear_session(&mut self) {
        self.keys = None;
        self.send_ctr = 0;
        self.replay = ReplayWindow::window();
        self.initiator = None;
        self.refused = false;
        self.peer_not_secure_raised = false;
        self.pending = None;
        self.half_open = None;
        self.auth = None;
        self.refusal_owed = None;
        self.last_refusal = None;
    }

    /// RAM held beyond the struct itself.
    pub(super) fn heap_bytes() -> usize {
        MAX_EVENTS * size_of::<SecureEvent>()
    }
}

/// The ephemeral public key at the head of a msg1.
pub(super) fn ephemeral_of(msg1: &[u8; MSG1_LEN]) -> [u8; 32] {
    let mut e = [0u8; 32];
    e.copy_from_slice(&msg1[..32]);
    e
}
