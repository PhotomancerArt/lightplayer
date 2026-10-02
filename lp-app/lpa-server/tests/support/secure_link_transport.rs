//! A `ServerTransport` over one secure lp-link responder: the device's end
//! of a secure network link, for host tests (plan
//! `lp2025/2026-10-01-1843-secure-link`, Q8). M7 may promote it to a real
//! module when its relay work needs a host stand-in for a device.
//!
//! The responder is lp-link's own `Link` built with `new_secure`; this
//! transport forwards its handshake to the server
//! (`take_secure_events` / `answer_key_lookup`) and reports the link
//! `LinkTrust::Keyed`. Each lp-link session is its own server link: a
//! `Reset` closes the `LinkId` (the server drops its session and grant) and
//! the next session gets a new one. A key lookup belongs to the session its
//! handshake will open, so it carries that session's `LinkId`: while a
//! session is up, a new msg1 is for the NEXT session (the responder resets
//! only once it verifies), and its lookup, refusal or grant names the next
//! id, never the live one.
//!
//! The pipe is the caller's: frames out of `poll_transmit`, bytes in through
//! `on_bytes` (a reliable, ordered pipe, as a WebSocket is).

use std::vec::Vec;

use lpc_shared::transport::{
    Incoming, KeyAnswer, Link, LinkId, LinkTrust, SecureLinkEvent, ServerTransport,
};
use lpc_wire::lp_link::secure_channel::{KeyId, Psk, RefusalReason, SecureEvent, SecureRole};
use lpc_wire::lp_link::{CH_PROTO, Link as LpLink, LinkConfig, LinkEvent, Micros, SelectiveRepeat};
use lpc_wire::{TransportError, WireServerMessage, decode_client_payload};

pub struct SecureLinkTransport {
    pub board: LpLink<SelectiveRepeat>,
    /// The live session's server link (valid while `up`).
    link_id: LinkId,
    /// The server link the next handshake's session will be.
    handshake_id: LinkId,
    next_id: u32,
    up: bool,
    /// The key id of the lookup the server is answering.
    lookup: Option<KeyId>,
    events: Vec<(LinkId, SecureLinkEvent)>,
    closed: Vec<LinkId>,
    inbox: Vec<Incoming>,
    came_up: Vec<LinkId>,
}

impl SecureLinkTransport {
    pub fn new(config: LinkConfig, nonce: u32, entropy: fn(&mut [u8])) -> Self {
        SecureLinkTransport {
            board: LpLink::new_secure(config, nonce, SecureRole::Responder, entropy),
            link_id: LinkId::new(1000),
            handshake_id: LinkId::new(1000),
            next_id: 1001,
            up: false,
            lookup: None,
            events: Vec::new(),
            closed: Vec::new(),
            inbox: Vec::new(),
            came_up: Vec::new(),
        }
    }

    /// The current session's server link.
    pub fn link(&self) -> Link {
        Link {
            id: self.link_id,
            trust: LinkTrust::Keyed,
        }
    }

    /// Bytes from the client.
    pub fn on_bytes(&mut self, now: Micros, bytes: &[u8]) {
        self.board.on_bytes(now, bytes);
        self.pump();
    }

    /// The next frame for the client.
    pub fn poll_transmit(&mut self, now: Micros) -> Option<Vec<u8>> {
        let frame = self.board.poll_transmit(now).map(<[u8]>::to_vec);
        self.pump();
        frame
    }

    /// Server links that came up since the last call: the edge sends each
    /// its hello now (the board's first proto message on every `Up`).
    pub fn take_came_up(&mut self) -> Vec<LinkId> {
        core::mem::take(&mut self.came_up)
    }

    /// Queue `msg` on the session now (the edge's hello).
    pub fn send_now(&mut self, msg: &WireServerMessage) {
        let json = lpc_wire::json::to_string(msg).expect("a server message serializes");
        self.board
            .send(CH_PROTO, json.as_bytes())
            .expect("the link takes a hello");
    }

    fn fresh_id(&mut self) -> LinkId {
        let id = LinkId::new(self.next_id);
        self.next_id += 1;
        id
    }

    fn pump(&mut self) {
        while let Some(event) = self.board.poll_secure_event() {
            match event {
                SecureEvent::KeyLookup { key_id } => {
                    if self.up && self.handshake_id == self.link_id {
                        self.handshake_id = self.fresh_id();
                    }
                    self.lookup = Some(key_id);
                    self.events.push((
                        self.handshake_id,
                        SecureLinkEvent::KeyLookup { salt: key_id.0 },
                    ));
                }
                SecureEvent::WrongKey { key_id } => {
                    self.events.push((
                        self.handshake_id,
                        SecureLinkEvent::WrongKey { salt: key_id.0 },
                    ));
                }
                SecureEvent::Refused { .. } | SecureEvent::PeerNotSecure => {}
            }
        }
        while let Some(event) = self.board.recv() {
            match event {
                LinkEvent::Up { .. } => {
                    self.link_id = self.handshake_id;
                    self.up = true;
                    if let Some(auth) = self.board.session_auth() {
                        self.events.push((
                            self.link_id,
                            SecureLinkEvent::Authenticated {
                                salt: auth.key_id.0,
                                candidate: auth.candidate,
                            },
                        ));
                    }
                    self.came_up.push(self.link_id);
                }
                LinkEvent::Reset { .. } => {
                    // A new lp-link session is a new server link: the old
                    // one's session and grant go with it.
                    if self.up {
                        self.closed.push(self.link_id);
                        if self.handshake_id == self.link_id {
                            self.handshake_id = self.fresh_id();
                        }
                    }
                    self.up = false;
                }
                LinkEvent::Message {
                    channel: CH_PROTO,
                    data,
                } => {
                    let msg = decode_client_payload(&data).expect("a client request");
                    self.inbox.push(Incoming {
                        link: self.link_id,
                        trust: LinkTrust::Keyed,
                        msg,
                    });
                }
                LinkEvent::Message { .. } | LinkEvent::Text(_) => {}
            }
        }
    }
}

impl ServerTransport for SecureLinkTransport {
    async fn send(&mut self, link: LinkId, msg: WireServerMessage) -> Result<(), TransportError> {
        if link != self.link_id || !self.up {
            // A reply to a session that ended: lost with it, as on a board.
            return Ok(());
        }
        self.send_now(&msg);
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<Incoming>, TransportError> {
        Ok(if self.inbox.is_empty() {
            None
        } else {
            Some(self.inbox.remove(0))
        })
    }

    async fn receive_all(&mut self) -> Result<Vec<Incoming>, TransportError> {
        Ok(core::mem::take(&mut self.inbox))
    }

    fn links(&self) -> Vec<Link> {
        if self.up {
            vec![self.link()]
        } else {
            Vec::new()
        }
    }

    fn take_closed_links(&mut self) -> Vec<LinkId> {
        core::mem::take(&mut self.closed)
    }

    fn take_secure_events(&mut self) -> Vec<(LinkId, SecureLinkEvent)> {
        core::mem::take(&mut self.events)
    }

    fn answer_key_lookup(&mut self, link: LinkId, answer: KeyAnswer) {
        if link != self.handshake_id {
            return;
        }
        let Some(key_id) = self.lookup.take() else {
            return;
        };
        match answer {
            KeyAnswer::Keys(psks) => {
                let psks: Vec<Psk> = psks.into_iter().map(Psk::new).collect();
                self.board.provide_keys(key_id, &psks);
            }
            KeyAnswer::Unknown => self.board.refuse(key_id, RefusalReason::UnknownKey, 0),
            KeyAnswer::Backoff { retry_after_ms } => {
                self.board
                    .refuse(key_id, RefusalReason::Backoff, retry_after_ms)
            }
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}
