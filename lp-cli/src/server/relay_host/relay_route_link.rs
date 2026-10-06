//! One relay route on the host board: a secure lp-link responder.
//!
//! Promoted from `lpa-server`'s test-only `SecureLinkTransport` (its doc
//! said M7 would), cut to one route: the board's end of one browser
//! session through the relay. Its handshake goes to the server
//! (`SecureLinkEvent`s, answered with `answer_key_lookup`), and its server
//! link is `LinkTrust::Relayed`, so the board's open setting never applies.
//! Each lp-link session is its own server link; a session that resets is
//! closed, as a board closes a keyed link whose session resets.

use std::collections::VecDeque;
use std::time::Instant;

use lpa_client::transport_lan::os_entropy;
use lpc_shared::transport::{Incoming, KeyAnswer, Link, LinkId, LinkTrust, SecureLinkEvent};
use lpc_wire::lp_link::secure_channel::{KeyId, Psk, RefusalReason, SecureEvent, SecureRole};
use lpc_wire::lp_link::{
    CH_PROTO, Link as LpLink, LinkConfig, LinkEvent, Micros, SelectiveRepeat, SendError,
};
use lpc_wire::{WireServerMessage, decode_client_payload};

/// What one pump of a route produced for the transport.
#[derive(Default)]
pub struct RouteOutput {
    /// lp-link frames for the browser, in order.
    pub frames: Vec<Vec<u8>>,
    /// Requests from the client.
    pub inbox: Vec<Incoming>,
    /// Handshake events for the server.
    pub events: Vec<(LinkId, SecureLinkEvent)>,
    /// Server links that closed.
    pub closed: Vec<LinkId>,
    /// Server links that came up (each is owed its hello).
    pub came_up: Vec<LinkId>,
    /// The session reset after it was up: the route should close.
    pub reset: bool,
}

/// See the module doc.
pub struct RelayRouteLink {
    link: LpLink<SelectiveRepeat>,
    started: Instant,
    /// The live session's server link (valid while `up`).
    link_id: LinkId,
    /// The server link the next handshake's session will be.
    handshake_id: LinkId,
    up: bool,
    /// The key id of the lookup the server is answering.
    lookup: Option<KeyId>,
    /// Messages for the client the link had no room for yet.
    outbox: VecDeque<Vec<u8>>,
}

impl RelayRouteLink {
    /// A responder whose first session will be server link `first`.
    pub fn new(first: LinkId, nonce: u32) -> Self {
        Self {
            link: LpLink::new_secure(LinkConfig::ws(), nonce, SecureRole::Responder, os_entropy),
            started: Instant::now(),
            link_id: first,
            handshake_id: first,
            up: false,
            lookup: None,
            outbox: VecDeque::new(),
        }
    }

    /// The live session's server link, if one is up.
    pub fn link(&self) -> Option<Link> {
        self.up.then_some(Link {
            id: self.link_id,
            trust: LinkTrust::Relayed,
        })
    }

    /// Whether `link` is this route's live session or its pending handshake.
    pub fn owns(&self, link: LinkId) -> bool {
        link == self.handshake_id || (self.up && link == self.link_id)
    }

    /// One lp-link frame from the browser.
    pub fn on_frame(&mut self, frame: &[u8]) {
        let now = self.now();
        self.link.on_datagram(now, frame);
    }

    /// Queue `msg` for the live session (a reply, or its hello).
    pub fn send(&mut self, link: LinkId, msg: &WireServerMessage) {
        if !self.up || link != self.link_id {
            // A reply to a session that ended: lost with it, as on a board.
            return;
        }
        match lpc_wire::json::to_string(msg) {
            Ok(json) => self.outbox.push_back(json.into_bytes()),
            Err(error) => log::warn!("relay route: a server message did not serialize: {error}"),
        }
    }

    /// The server's answer to this route's key lookup.
    pub fn answer_key_lookup(&mut self, link: LinkId, answer: KeyAnswer) {
        if link != self.handshake_id {
            return;
        }
        let Some(key_id) = self.lookup.take() else {
            return;
        };
        match answer {
            KeyAnswer::Keys(psks) => {
                let psks: Vec<Psk> = psks.into_iter().map(Psk::new).collect();
                self.link.provide_keys(key_id, &psks);
            }
            KeyAnswer::Unknown => self.link.refuse(key_id, RefusalReason::UnknownKey, 0),
            KeyAnswer::Backoff { retry_after_ms } => {
                self.link
                    .refuse(key_id, RefusalReason::Backoff, retry_after_ms);
            }
        }
    }

    /// Run the link: queued messages in, events out, frames out. `next_id`
    /// mints the server link of a session after this one.
    pub fn pump(&mut self, next_id: &mut impl FnMut() -> LinkId) -> RouteOutput {
        let mut out = RouteOutput::default();
        while let Some(message) = self.outbox.front() {
            match self.link.send(CH_PROTO, message) {
                Ok(()) => {
                    self.outbox.pop_front();
                }
                Err(SendError::Full) => break,
                Err(_) => {
                    log::warn!("relay route: a message larger than the link carries was dropped");
                    self.outbox.pop_front();
                }
            }
        }
        while let Some(event) = self.link.poll_secure_event() {
            match event {
                SecureEvent::KeyLookup { key_id } => {
                    if self.up && self.handshake_id == self.link_id {
                        self.handshake_id = next_id();
                    }
                    self.lookup = Some(key_id);
                    out.events.push((
                        self.handshake_id,
                        SecureLinkEvent::KeyLookup { salt: key_id.0 },
                    ));
                }
                SecureEvent::WrongKey { key_id } => out.events.push((
                    self.handshake_id,
                    SecureLinkEvent::WrongKey { salt: key_id.0 },
                )),
                SecureEvent::Refused { .. } | SecureEvent::PeerNotSecure => {}
            }
        }
        while let Some(event) = self.link.recv() {
            match event {
                LinkEvent::Up { .. } => {
                    if self.up {
                        // A second session replaced the first.
                        out.closed.push(self.link_id);
                    }
                    self.link_id = self.handshake_id;
                    self.up = true;
                    if let Some(auth) = self.link.session_auth() {
                        out.events.push((
                            self.link_id,
                            SecureLinkEvent::Authenticated {
                                salt: auth.key_id.0,
                                candidate: auth.candidate,
                            },
                        ));
                    }
                    out.came_up.push(self.link_id);
                }
                LinkEvent::Reset { .. } => {
                    if self.up {
                        out.closed.push(self.link_id);
                        out.reset = true;
                    }
                    self.up = false;
                }
                LinkEvent::Message {
                    channel: CH_PROTO,
                    data,
                } => match decode_client_payload(&data) {
                    Ok(msg) => out.inbox.push(Incoming {
                        link: self.link_id,
                        trust: LinkTrust::Relayed,
                        msg,
                    }),
                    Err(error) => log::warn!("relay route: a request did not parse: {error}"),
                },
                LinkEvent::Message { .. } | LinkEvent::Text(_) => {}
            }
        }
        let now = self.now();
        while let Some(frame) = self.link.poll_transmit(now) {
            out.frames.push(frame.to_vec());
        }
        out
    }

    /// The route is gone: its live session's server link, if any, to close.
    pub fn close(self) -> Option<LinkId> {
        self.up.then_some(self.link_id)
    }

    fn now(&self) -> Micros {
        u64::try_from(self.started.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
}
