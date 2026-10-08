//! The link mux: one [`ServerTransport`] over the USB cable and up to
//! [`LINK_SLOTS`] radio links — Bluetooth's, and with feature `wifi` the
//! LAN's.
//!
//! - **USB** is the primary transport it wraps ([`LinkId::PRIMARY`],
//!   trusted), unchanged: everything addressed to the primary link goes
//!   straight to it.
//! - **Radio links** arrive and leave through the [`RadioLinkPort`], one
//!   [`LinkId`] each, all untrusted. Each is one lp-link session per
//!   connection (Datagram framing, one frame per ATT operation, the port's
//!   `Link`): the mux takes whole wire messages off the link's proto channel
//!   ([`crate::serial::server_payload::decode_client_payload`], the USB
//!   link's decoder) and queues replies onto it
//!   ([`crate::serial::server_payload::serialize_server_payload`], the USB
//!   link's encoder). The server's gate decides what an untrusted link may
//!   do; this file only decides which bytes go where.
//!
//! Five rules live here because this is the edge that owns them:
//!
//! 1. **One frame buffer, three holders.** Every reply is serialized into the
//!    same static frame buffer (`serial::server_msg`) — the ADR's "no second
//!    16 KiB buffer per link" (`docs/adr/2026-09-24-ble-transport.md`,
//!    decision 3). A long reply stays there as an lp-link *external* message
//!    while its link cuts it into frames (`Link::send_external`), on USB and
//!    on each radio link alike, so before anything is serialized into it
//!    every other holder lets go: each radio link that still reads it, then
//!    the USB link ([`FrameBufHolder`]). A reply of at most
//!    [`SMALL_REPLY_BYTES`] is copied into its radio link's own send ring
//!    instead, and holds nothing. The core's update hook (rule 5) writes the
//!    buffer too, for a read-back chunk, and only while no link holds it.
//! 2. **A slow radio link cannot stall the device for long.** The wait for a
//!    radio link to let go of the frame buffer is bounded — by its own
//!    deadline ([`RADIO_WRITE_DEADLINE_MS`] on Bluetooth,
//!    [`LAN_WRITE_DEADLINE_MS`] on the LAN) and by what is left of the
//!    tick's own budget ([`TICK_WAIT_LIMIT_MS`], measured from the last
//!    upkeep, so a tick that already compiled a shader cannot then wait out
//!    a whole deadline and trip the watchdog); past it the link is closed
//!    with a logged reason (its link dropped at once, so it reads the
//!    buffer no more) and the server loop moves on.
//! 3. **A radio link's hello waits for its lp-link session.** The radio side
//!    announces a link when the central enables notifications (it could
//!    receive nothing before); its lp-link handshake runs after that, as
//!    ordinary traffic. The hello is owed when that session comes `Up` —
//!    and again on every later `Up` — and nothing of a session is taken or
//!    answered before its hello, exactly the USB link's rule. On `Up` and on
//!    `Reset` the link's replies go back to JSON and a `Reset` drops the
//!    requests of the session it ended.
//! 4. **A radio link must log in within [`LOGIN_DEADLINE_MS`]** of opening,
//!    unless the device is `open` — [`LinkMuxTransport::expire_unauthenticated`],
//!    driven from the server loop, which holds the clock and the server.
//!    While the link's own `LoginBegin` challenge is outstanding the deadline
//!    waits for it — a person is typing a password — but never past that
//!    challenge's expiry (`lpc_access::CHALLENGE_TTL_MS`), which the server
//!    enforces: once the challenge expires or its answer is refused, the
//!    ordinary deadline applies again, and a link already past it closes.
//! 5. **Channel 3 goes to the core with the link's granted tier.** A radio
//!    link's update-protocol message (`lpc-update`) is queued as it is
//!    taken and handed to the core's update hook
//!    ([`LinkMuxTransport::with_update_hook`],
//!    [`super::radio_update_channel`]) from the upkeep, which holds the
//!    server: with the tier a login or key **granted** the link
//!    (`LpServer::link_granted_tier`), never the device's `open` setting —
//!    the board session adds that itself, so its one access rule decides.
//!    The mux holds no copy of that rule. A closed link is passed on so the
//!    session forgets it, its queued messages dropped; a session `Reset`
//!    drops the ended session's. With no hook (a monolithic image, DM25)
//!    channel 3 is ignored. A Bluetooth link's channel 3 goes to the hook,
//!    and a LAN link's, with the tier its key granted it on the server; a
//!    relayed link's is ignored until updates through the relay are their
//!    own change.
//!
//! **Network links** (feature `wifi`: the LAN's, and the cloud relay's
//! routes) ride the same slots and the same rules, with five differences:
//! they are secure lp-link responders, so they are [`LinkTrust::Keyed`] (or
//! [`LinkTrust::Relayed`] through the relay) and their handshake asks the
//! server for keys ([`ServerTransport::take_secure_events`] /
//! [`ServerTransport::answer_key_lookup`]; the key that verifies decides the
//! link's tier at `Up`); a secure session that resets after coming up
//! closes the link (a new session is a new server link, with a new grant, so
//! the client reconnects); they are served from another thread, under the
//! port's lock (see `radio_link_port`); a relayed link's channel 3 is not
//! served yet (rule 5); and the LAN and the relay share **one** network slot (Wi-Fi relay
//! plan D2). A newcomer that finds it held is a *challenge*
//! ([`RadioLinkEvent::Challenged`]): it takes the slot only with a handshake
//! that verifies under the holder's own key, looked up through the server
//! like any other, and anything else is told busy without a lookup
//! (`parked_handshake` has the rule and why).
//!
//! A radio send that fails does **not** return an error to the server. The
//! server's `tick_and_send` stops answering the whole batch on the first
//! transport error, and one dying radio link must not cost the USB cable its
//! replies. The failure is logged once, at error level, with the link and the
//! reason, and the link is closed — its session is dropped on the next tick
//! ([`ServerTransport::take_closed_links`]); frames still addressed to it are
//! skipped at debug level.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use embassy_futures::select::{Either, select};
use embedded_hal_async::delay::DelayNs;
use lp_link::{CH_PROTO, CH_UPDATE, LinkEvent, LinkState, Micros, ResetReason, SendError};
use lpa_server::LpServer;
use lpc_access::{OpenTo, Tier};
use lpc_shared::transport::{Incoming, Link, LinkId, ServerTransport};
#[cfg(feature = "wifi")]
use lpc_shared::transport::{KeyAnswer, LinkTrust, SecureLinkEvent};
use lpc_wire::server::ServerMsgBody;
use lpc_wire::{LinkCounterTally, TransportError, WireServerMessage};

use super::frame_buf_holder::FrameBufHolder;
#[cfg(feature = "wifi")]
use super::network_key_answer::answer_key_lookup;
#[cfg(feature = "wifi")]
use super::parked_handshake::Msg1;
use super::radio_link_config::SMALL_REPLY_BYTES;
#[cfg(feature = "wifi")]
use super::radio_link_port::RADIO_LINK_SLOTS;
use super::radio_link_port::{LINK_SLOTS, RadioLinkEvent, RadioLinkPort, RadioLinkSlot};
use super::radio_update_channel::{RadioUpdate, RadioUpdateHook};
use crate::link_upkeep::LinkUpkeep;
use crate::serial::packed_link::PackedLink;
use crate::serial::server_msg::frame_bytes;
use crate::serial::server_payload::{
    decode_client_payload, request_refusal, serialize_server_payload,
};

/// How long a radio link has to finish taking a long reply out of the frame
/// buffer once someone else needs the buffer, before it is closed.
///
/// The largest reply is the 16 KiB project-read budget. The slowest central
/// measured (a MacBook, spike Run C) took notifications at 5–12 KB/s, so the
/// worst honest reply needs ~3.3 s; the bound is set above that, not at it.
pub const RADIO_WRITE_DEADLINE_MS: u32 = 5_000;

/// The same bound for a LAN link. A LAN peer drains a whole 16 KiB reply in
/// milliseconds, so one that has not let go of the buffer in a second has
/// stopped reading (a backgrounded tab, a stalled Wi-Fi), and every second
/// of waiting is a second the board does not render.
///
/// A **relayed** link is not a LAN peer: every window crosses the internet
/// twice. PR A measured request round trips of 215–224 ms through the relay
/// with 100 ms added each way, and a 16 KiB reply at the board's window of
/// two 1 KB frames is eight round trips (~1.8 s), so a relayed link gets the
/// slow path's bound, [`RADIO_WRITE_DEADLINE_MS`].
pub const LAN_WRITE_DEADLINE_MS: u32 = 1_000;

/// The most one server tick may spend waiting for radio links to let go of
/// the frame buffer, counted from the tick's start (the last upkeep) and
/// including whatever the tick did first — a project load, a shader
/// compile. Below the 8 s watchdog with room for the rest of the tick: a
/// load followed by a stalled peer's 5 s wait reset the emulated C6 in a
/// loop (PR C's walk, 12 watchdog resets).
pub const TICK_WAIT_LIMIT_MS: u32 = 5_000;

/// How long an untrusted radio link may stay open without logging in (PQ6).
pub const LOGIN_DEADLINE_MS: u64 = 10_000;

/// Client messages the inbox holds without growing: two per link, a
/// request and the next one queued behind it. The server takes one per
/// tick, so more is a burst, and the inbox grows for it as it always has.
const INBOX_RESERVE: usize = 2 * LINK_SLOTS;

/// A LAN link's opt-in answer is always `json`: its replies are never
/// packed. A learned table is 6.9 KB per link, held for the link's life,
/// and a LAN link has the bandwidth packing exists to save (a Wi-Fi link
/// moves a JSON project read in a few frames). Hosts take a `json` answer
/// as any board's decline and keep reading JSON.
#[cfg(feature = "wifi")]
fn stay_json(answer: &mut ServerMsgBody) {
    if let ServerMsgBody::SetEncoding { encoding } = answer {
        *encoding = lpc_wire::WireEncoding::Json;
    }
}

/// One open radio link, as the mux tracks it.
struct RadioLink {
    id: LinkId,
    slot: usize,
    /// The link as the server sees it: untrusted (Bluetooth) or keyed (LAN).
    wire: Link,
    /// A keyed link's handshake asked for this key id; the server's answer
    /// goes back to it.
    #[cfg(feature = "wifi")]
    pending_key: Option<lp_link::secure_channel::KeyId>,
    /// Server-loop time the link was first seen by the upkeep, which starts
    /// its login deadline.
    opened_at_ms: Option<u64>,
    /// It held a tier once; the login deadline no longer applies.
    cleared: bool,
    /// Its lp-link session has come `Up` at least once. A link that never
    /// does is closed at the login deadline whatever tier it would hold.
    ever_up: bool,
    /// The lp-link session the mux last saw come `Up`, whose hello has gone
    /// out (or is owed): replies are coded for it and for no other.
    session: Option<u32>,
    /// The session came up and its hello has not been handed out yet.
    hello_owed: bool,
    /// What this link's replies are written in, and its learned table while
    /// packed: JSON until the server answers its `SetEncoding` opt-in, then
    /// what that answer names — the USB link's rule (plan `lp-json-pack`).
    /// JSON again on every `Up` and `Reset`; a radio link that closes takes
    /// its encoding and table with it.
    packed: PackedLink,
    /// Resets by reason, stalls and payload errors, for this link's
    /// heartbeat.
    tally: LinkCounterTally,
}

/// A newcomer whose first frame is parked on the held network slot.
#[cfg(feature = "wifi")]
struct Challenge {
    id: LinkId,
    slot: usize,
    msg1: Msg1,
}

/// What queueing one reply on a radio link came to.
enum Queued {
    Yes,
    /// The session the reply was coded for is gone (a reset, or not up).
    NoSession,
    /// The link would not take it (cannot happen once the frame buffer is
    /// released; closed as a failed write if it does).
    Refused(SendError),
}

/// The USB transport plus the radio links, as one [`ServerTransport`].
pub struct LinkMuxTransport<U, D> {
    primary: U,
    port: &'static RadioLinkPort,
    delay: D,
    radio: Vec<RadioLink>,
    /// Client messages taken off the radio links, waiting for `receive`.
    inbox: VecDeque<Incoming>,
    /// Closed links the server has not been told about yet.
    closed: Vec<LinkId>,
    /// Keyed links' handshake events the server has not taken yet.
    #[cfg(feature = "wifi")]
    secure: Vec<(LinkId, SecureLinkEvent)>,
    /// The network slot's challenger, while the server looks its key up.
    #[cfg(feature = "wifi")]
    challenge: Option<Challenge>,
    upkeep_hook: Option<fn(&LpServer, u64)>,
    /// Until when the current server tick may wait on radio links: the last
    /// upkeep (the end of the previous tick) plus [`TICK_WAIT_LIMIT_MS`].
    tick_wait_until: Micros,
    /// The core's channel-3 hook (rule 5); `None`: channel 3 is ignored.
    update_hook: Option<RadioUpdateHook>,
    /// Channel-3 messages taken off the Bluetooth links, waiting for the
    /// upkeep.
    updates: VecDeque<(LinkId, Vec<u8>)>,
    /// Closed links the update hook has not been told about yet.
    updates_closed: Vec<LinkId>,
}

impl<U: ServerTransport + FrameBufHolder, D: DelayNs> LinkMuxTransport<U, D> {
    /// Wrap `primary` (the USB transport) and serve the radio links that
    /// `port` announces. `delay` bounds a radio link's hold on the frame
    /// buffer.
    ///
    /// Every list the mux keeps per link is reserved here, at its most
    /// links ([`LINK_SLOTS`]), so none of them grows when a link opens: a
    /// growth then would land above whatever the link allocated and outlive
    /// it, splitting the hole the link leaves when it closes
    /// (`docs/defects/2026-10-06-a-lan-link-strands-the-heap-below-the-load-floor.md`).
    pub fn new(primary: U, port: &'static RadioLinkPort, delay: D) -> Self {
        Self {
            primary,
            port,
            delay,
            radio: Vec::with_capacity(LINK_SLOTS),
            inbox: VecDeque::with_capacity(INBOX_RESERVE),
            closed: Vec::with_capacity(LINK_SLOTS),
            #[cfg(feature = "wifi")]
            secure: Vec::with_capacity(LINK_SLOTS + 1),
            #[cfg(feature = "wifi")]
            challenge: None,
            upkeep_hook: None,
            tick_wait_until: tick_wait_until(),
            update_hook: None,
            updates: VecDeque::with_capacity(LINK_SLOTS),
            updates_closed: Vec::with_capacity(LINK_SLOTS),
        }
    }

    /// Hand each radio link's channel-3 messages to `hook` — the core's
    /// update session — with the tier the link was granted (rule 5). A split
    /// image's engine installs it; a monolithic one does not.
    #[must_use]
    pub fn with_update_hook(mut self, hook: RadioUpdateHook) -> Self {
        self.update_hook = Some(hook);
        self
    }

    /// Hand the queued channel-3 messages to the update hook, each with the
    /// tier `granted` says its link holds by a login or a key
    /// (`LpServer::link_granted_tier`) and the device's `open` setting as it
    /// stands now (`LpServer::device_open`, asked only when a message
    /// waits), after telling it which links closed; then one pass, to flush
    /// what it holds. Driven from the upkeep, which holds the server.
    pub fn dispatch_updates(
        &mut self,
        granted: impl Fn(Link) -> Option<Tier>,
        open: impl FnOnce() -> OpenTo,
    ) {
        let Some(hook) = self.update_hook else {
            return;
        };
        for link in self.updates_closed.drain(..) {
            hook(RadioUpdate::Closed { link });
        }
        let open = (!self.updates.is_empty()).then(open);
        while let Some((link, bytes)) = self.updates.pop_front() {
            // The link as the server knows it on its slot: keyed on the
            // LAN, untrusted on Bluetooth.
            let wire = self
                .radio
                .iter()
                .find(|l| l.id == link)
                .map_or(RadioLinkPort::link(link), |l| l.wire);
            hook(RadioUpdate::Message {
                link,
                granted: granted(wire),
                open: open.unwrap_or(OpenTo::Nobody),
                bytes: &bytes,
            });
        }
        hook(RadioUpdate::Pass);
    }

    /// `link` is gone: its queued channel-3 messages go, and the update hook
    /// is owed a closed notice.
    fn forget_updates_of(&mut self, link: LinkId) {
        self.updates.retain(|(l, _)| *l != link);
        if self.update_hook.is_some() {
            self.updates_closed.push(link);
        }
    }

    /// Also run `hook` on every upkeep — the radio edge's own periodic work
    /// that needs the server (the BLE task's advertised name follows the
    /// loaded project).
    #[must_use]
    pub fn with_upkeep_hook(mut self, hook: fn(&LpServer, u64)) -> Self {
        self.upkeep_hook = Some(hook);
        self
    }

    /// Close every radio link that has been open for [`LOGIN_DEADLINE_MS`]
    /// without holding a tier, unless its own login challenge is still
    /// outstanding. `now_ms` is the server loop's clock; `has_tier` and
    /// `login_pending` ask the server (`LpServer::link_tier(..).is_some()`,
    /// `LpServer::login_pending`).
    pub fn expire_unauthenticated(
        &mut self,
        now_ms: u64,
        has_tier: impl Fn(Link) -> bool,
        login_pending: impl Fn(Link) -> bool,
    ) {
        let mut expired = Vec::new();
        let mut never_up = Vec::new();
        for link in &mut self.radio {
            let opened_at = *link.opened_at_ms.get_or_insert(now_ms);
            let late = now_ms.saturating_sub(opened_at) >= LOGIN_DEADLINE_MS;
            // A link whose session never came up holds a slot and speaks
            // nothing: on an open board it "holds a tier" from its first
            // frame, so the login deadline never applied, and with one LAN
            // slot a peer that opened a socket and went quiet (a page torn
            // down mid-reload) kept the next client out (PR C's walk:
            // `frames in 0 out 534 · handshakes 0`).
            if !link.ever_up && late {
                never_up.push(link.id);
                continue;
            }
            if link.cleared {
                continue;
            }
            let wire_link = link.wire;
            if has_tier(wire_link) {
                link.cleared = true;
            } else if now_ms.saturating_sub(opened_at) >= LOGIN_DEADLINE_MS
                && !login_pending(wire_link)
            {
                expired.push(link.id);
            }
        }
        for id in expired {
            log::warn!(
                "radio link {id}: no login within {} s — closing",
                LOGIN_DEADLINE_MS / 1000
            );
            self.close_radio(id, "no login within the deadline");
        }
        for id in never_up {
            log::warn!(
                "radio link {id}: its session never came up in {} s — closing",
                LOGIN_DEADLINE_MS / 1000
            );
            self.close_radio(id, "no session within the deadline");
        }
    }

    /// Radio links currently open.
    #[must_use]
    pub fn radio_link_count(&self) -> usize {
        self.radio.len()
    }

    // Out of line (this and the pump below): their locals — a delivered
    // message, a decoded request — stay in their own frames and never join
    // the server loop future's, which every deep call stacks on.
    #[inline(never)]
    fn drain_events(&mut self) {
        while let Some(event) = self.port.try_event() {
            match event {
                RadioLinkEvent::Opened { link, slot } => {
                    if slot >= LINK_SLOTS || self.radio.iter().any(|l| l.slot == slot) {
                        // Cannot happen with a well-behaved radio side; if it
                        // does, refuse the newcomer rather than cross wires.
                        log::error!("radio link {link}: slot {slot} unusable — closing");
                        if slot < LINK_SLOTS {
                            self.port.slot(slot).request_close("slot already in use");
                        }
                        continue;
                    }
                    log::info!("radio link {link}: opened (slot {slot})");
                    self.radio.push(RadioLink {
                        id: link,
                        slot,
                        wire: self.port.link_on(slot, link),
                        #[cfg(feature = "wifi")]
                        pending_key: None,
                        opened_at_ms: None,
                        cleared: false,
                        ever_up: false,
                        session: None,
                        hello_owed: false,
                        packed: PackedLink::new(),
                        tally: LinkCounterTally::new(),
                    });
                }
                RadioLinkEvent::Closed { link } => {
                    if let Some(at) = self.radio.iter().position(|l| l.id == link) {
                        let gone = self.radio.remove(at);
                        self.inbox.retain(|i| i.link != link);
                        self.forget_updates_of(link);
                        self.closed.push(link);
                        log::info!("radio link {link}: closed");
                        // The holder left while a newcomer waited on its
                        // key: the newcomer is told busy and simply tries
                        // again, onto a free slot.
                        #[cfg(feature = "wifi")]
                        if self.challenge.as_ref().is_some_and(|c| c.slot == gone.slot) {
                            self.refuse_challenge("its holder left");
                        }
                        #[cfg(not(feature = "wifi"))]
                        let _ = gone;
                    }
                    #[cfg(feature = "wifi")]
                    if self.challenge.as_ref().is_some_and(|c| c.id == link) {
                        // Its edge gave up waiting.
                        self.challenge = None;
                        self.closed.push(link);
                    }
                }
                RadioLinkEvent::Challenged { link, slot } => {
                    #[cfg(feature = "wifi")]
                    self.challenged(link, slot);
                    #[cfg(not(feature = "wifi"))]
                    log::error!("radio link {link}: a challenge on slot {slot} with no network");
                }
            }
        }
    }

    /// A newcomer parked its first frame on the held network `slot`: ask
    /// the server for its key only when it names the holder's own key id
    /// (see `parked_handshake`); anything else is busy at once, with no
    /// lookup and nothing charged.
    #[cfg(feature = "wifi")]
    fn challenged(&mut self, link: LinkId, slot: usize) {
        let port = self.port;
        if slot >= LINK_SLOTS || self.challenge.is_some() {
            port.slot(slot.min(LINK_SLOTS - 1)).refuse_challenge(link);
            return;
        }
        let Some(Some(msg1)) = port.slot(slot).parked_msg1(link) else {
            log::info!("network link {link}: a newcomer's first frame is not a handshake — busy");
            port.slot(slot).refuse_challenge(link);
            return;
        };
        let holder_key = self
            .radio
            .iter()
            .find(|l| l.slot == slot)
            .and_then(|holder| port.slot(slot).with_link(holder.id, |l| l.session_auth()))
            .flatten()
            .map(|auth| auth.key_id);
        match holder_key {
            Some(key) if key == msg1.key_id && !key.is_anonymous() => {
                log::info!("network link {link}: the holder's key asks for the slot — checking it");
                self.secure
                    .push((link, SecureLinkEvent::KeyLookup { salt: key.0 }));
                self.challenge = Some(Challenge {
                    id: link,
                    slot,
                    msg1,
                });
            }
            _ => {
                log::info!("network link {link}: the network link is in use — busy");
                port.slot(slot).refuse_challenge(link);
            }
        }
    }

    /// The server answered the challenger's key lookup: a msg1 that
    /// verifies under one of its candidates takes the slot (the holder is
    /// closed first), anything else is busy — a wrong key charged to the
    /// backoff like any failed guess.
    #[cfg(feature = "wifi")]
    fn decide_challenge(&mut self, answer: KeyAnswer) {
        let Some(challenge) = self.challenge.take() else {
            return;
        };
        let verified = match &answer {
            KeyAnswer::Keys(psks) => psks.iter().any(|psk| challenge.msg1.verifies_with(psk)),
            KeyAnswer::Unknown | KeyAnswer::Backoff { .. } => false,
        };
        let slot = self.port.slot(challenge.slot);
        if verified {
            if let Some(holder) = self.radio.iter().find(|l| l.slot == challenge.slot) {
                let holder = holder.id;
                log::info!(
                    "network link {holder}: taken over by link {} (the same key)",
                    challenge.id
                );
                self.close_radio(holder, "taken over by the same key");
            }
            slot.grant_challenge(challenge.id);
            return;
        }
        if matches!(answer, KeyAnswer::Keys(_)) {
            self.secure.push((
                challenge.id,
                SecureLinkEvent::WrongKey {
                    salt: challenge.msg1.key_id.0,
                },
            ));
        }
        log::info!(
            "network link {}: its key did not verify — busy",
            challenge.id
        );
        slot.refuse_challenge(challenge.id);
        self.closed.push(challenge.id);
    }

    /// Turn the waiting challenger away.
    #[cfg(feature = "wifi")]
    fn refuse_challenge(&mut self, why: &str) {
        if let Some(challenge) = self.challenge.take() {
            log::info!("network link {}: busy ({why})", challenge.id);
            self.port
                .slot(challenge.slot)
                .refuse_challenge(challenge.id);
            self.closed.push(challenge.id);
        }
    }

    /// Take each radio link's events: client messages into the inbox,
    /// lifecycle into the link's state. A link whose session just came up
    /// is taken no further until its hello is handed out (rule 3).
    #[inline(never)]
    fn pump_radio(&mut self) {
        let port = self.port;
        let now = now_us();
        #[cfg(feature = "wifi")]
        let mut reset_keyed: Vec<LinkId> = Vec::new();
        for radio in &mut self.radio {
            let slot = port.slot(radio.slot);
            #[cfg(feature = "wifi")]
            if radio.wire.trust.is_secure() {
                take_secure_events(slot, radio, &mut self.secure);
            }
            while !radio.hello_owed {
                let Some(Some(event)) = slot.with_link(radio.id, |link| link.recv()) else {
                    break;
                };
                match event {
                    LinkEvent::Message { channel, data } if channel == CH_PROTO => {
                        // A request the heap cannot decode is refused in
                        // words, never decoded into a reset.
                        if let Some((reply, reason)) = request_refusal(&data) {
                            drop(data);
                            log::warn!("radio link {}: {reason}", radio.id);
                            slot.with_link(radio.id, |link| {
                                let _ = link.send(CH_PROTO, &reply);
                            });
                            slot.ring();
                            continue;
                        }
                        let decoded = decode_client_payload(&data);
                        match decoded {
                            Ok(msg) => {
                                // The request's bytes go before the inbox can
                                // grow (the USB link's reason: a growth above
                                // them would pin their hole).
                                drop(data);
                                log::debug!("radio link {}: received id={}", radio.id, msg.id);
                                self.inbox.push_back(Incoming::on(radio.wire, msg));
                            }
                            Err(error) => {
                                radio.tally.note_payload_error();
                                let prefix = &data[..data.len().min(48)];
                                log::warn!(
                                    "radio link {}: dropping a {} B proto message ({error}); \
                                     prefix: {:?}",
                                    radio.id,
                                    data.len(),
                                    alloc::string::String::from_utf8_lossy(prefix)
                                );
                            }
                        }
                    }
                    LinkEvent::Message { channel, data }
                        if channel == CH_UPDATE
                            && self.update_hook.is_some()
                            && carries_updates(radio.wire) =>
                    {
                        // For the core, with the link's tier, from the
                        // upkeep (rule 5). A relayed link's falls to the
                        // arm below like any other channel.
                        self.updates.push_back((radio.id, data));
                    }
                    LinkEvent::Message { channel, data } => {
                        log::debug!(
                            "radio link {}: {} B on channel {channel} ignored",
                            radio.id,
                            data.len()
                        );
                    }
                    LinkEvent::Text(text) => {
                        // Datagram framing hands up no text; nothing to do
                        // with it if it ever did.
                        log::debug!("radio link {}: {} B of text ignored", radio.id, text.len());
                    }
                    LinkEvent::Up { generation } => {
                        // A keyed link's grant before its hello: the key the
                        // handshake verified decides its tier.
                        #[cfg(feature = "wifi")]
                        if radio.wire.trust.is_secure()
                            && let Some(Some(auth)) = slot.with_link(radio.id, |l| l.session_auth())
                        {
                            self.secure.push((
                                radio.id,
                                SecureLinkEvent::Authenticated {
                                    salt: auth.key_id.0,
                                    candidate: auth.candidate,
                                },
                            ));
                        }
                        radio.packed.back_to_json();
                        radio.session = Some(generation);
                        radio.ever_up = true;
                        radio.hello_owed = true;
                        log::info!("radio link {}: session {generation} up", radio.id);
                    }
                    LinkEvent::Reset { reason, generation } => {
                        #[cfg(feature = "wifi")]
                        if radio.wire.trust.is_secure() && radio.session.is_some() {
                            reset_keyed.push(radio.id);
                        }
                        radio.packed.back_to_json();
                        radio.session = None;
                        radio.tally.note_reset(reason);
                        // Requests from the ended session: its host fails
                        // them itself, and a reply would reach the next one.
                        let id = radio.id;
                        self.inbox.retain(|i| i.link != id);
                        self.updates.retain(|(l, _)| *l != id);
                        // In words: a `{:?}` prints nothing on the C6
                        // (`-Z fmt-debug=none`).
                        let why = match reason {
                            ResetReason::PeerRestarted => "peer restarted",
                            ResetReason::Requested => "requested",
                            ResetReason::RetryLimit => "retry limit",
                            ResetReason::ProtocolError => "protocol error",
                        };
                        match reason {
                            ResetReason::PeerRestarted | ResetReason::Requested => log::info!(
                                "radio link {id}: session reset ({why}); now {generation}"
                            ),
                            ResetReason::RetryLimit | ResetReason::ProtocolError => log::warn!(
                                "radio link {id}: session reset ({why}); now {generation}"
                            ),
                        }
                    }
                }
            }
            if let Some(stalled) = slot.with_link(radio.id, |link| link.is_stalled(now)) {
                radio.tally.note_stalled(stalled);
            }
        }
        #[cfg(feature = "wifi")]
        for id in reset_keyed {
            log::info!(
                "radio link {id}: keyed session reset — closing (a new session is a new link)"
            );
            self.close_radio(id, "secure session reset");
        }
    }

    /// Drop `id` from the mux, drop its link (it stops reading the frame
    /// buffer now), ask the edge that served it to disconnect it, and owe the
    /// server a closed notice.
    fn close_radio(&mut self, id: LinkId, reason: &'static str) {
        if let Some(at) = self.radio.iter().position(|l| l.id == id) {
            let link = self.radio.remove(at);
            self.port.slot(link.slot).revoke(id, reason);
            self.inbox.retain(|i| i.link != id);
            self.forget_updates_of(id);
            self.closed.push(id);
        }
    }

    /// Wait until no radio link reads the frame buffer (rule 1): each one
    /// with a long reply in flight gets its deadline ([`write_deadline_ms`])
    /// to finish cutting it into frames, cut short by what is left of the
    /// tick's [`TICK_WAIT_LIMIT_MS`], and is closed past that.
    async fn release_radio_holders(&mut self) {
        let port = self.port;
        for index in 0..LINK_SLOTS {
            let slot = port.slot(index);
            if !slot.external_in_flight() {
                continue;
            }
            let left_ms = (self.tick_wait_until.saturating_sub(now_us()) / 1_000) as u32;
            let trust = self
                .radio
                .iter()
                .find(|l| l.slot == index)
                .map(|l| l.wire.trust);
            let wait_ms = write_deadline_ms(index, trust).min(left_ms);
            let released = wait_ms > 0
                && matches!(
                    select(wait_for_release(slot), self.delay.delay_ms(wait_ms)).await,
                    Either::First(())
                );
            if released || !slot.external_in_flight() {
                continue;
            }
            let Some((id, held)) = slot.with_any_link(|id, link| (id, link.buffered_bytes()))
            else {
                continue;
            };
            log::error!(
                "radio link {id}: a reply still not out of the frame buffer after {wait_ms} ms \
                 ({held} B held; {left_ms} ms of the tick's wait budget left) — closing"
            );
            slot.drop_link(id);
            self.close_radio(id, "reply deadline");
        }
    }

    async fn send_radio(
        &mut self,
        link: LinkId,
        mut msg: WireServerMessage,
    ) -> Result<(), TransportError> {
        let Some(radio) = self.radio.iter().find(|l| l.id == link) else {
            log::debug!("radio link {link}: gone, skipping frame id={}", msg.id);
            return Ok(());
        };
        let is_hello = matches!(msg.msg, ServerMsgBody::Hello(_));
        let Some(session) = radio.session.filter(|_| !radio.hello_owed || is_hello) else {
            // Not up, or up with its hello still owed (rule 3): nothing of a
            // session goes before its hello, and there is no one to tell.
            log::debug!(
                "radio link {link}: no session yet, skipping frame id={}",
                msg.id
            );
            return Ok(());
        };
        // The frame buffer is every link's (rule 1): the other radio links
        // and the USB link may still be reading their last long reply.
        self.release_radio_holders().await;
        self.primary.release_frame_buf().await;
        let port = self.port;
        let Some(radio) = self.radio.iter_mut().find(|l| l.id == link) else {
            return Ok(());
        };
        let slot = port.slot(radio.slot);
        let id = msg.id;
        // An opt-in answer `packed` needs a table first; the answer itself is
        // always JSON (`table_for`), and the switch it announces applies to
        // every reply after it, until the session ends — as on USB.
        #[cfg(feature = "wifi")]
        if radio.slot >= RADIO_LINK_SLOTS {
            stay_json(&mut msg.msg);
        }
        radio.packed.prepare_answer(&mut msg.msg);
        let switch_to = match msg.msg {
            ServerMsgBody::SetEncoding { encoding } => Some(encoding),
            _ => None,
        };
        // The server fills a heartbeat's link counters from the USB link;
        // this link's own are the ones its host wants.
        if let ServerMsgBody::Heartbeat {
            link: Some(counters),
            ..
        } = &mut msg.msg
            && let Some(own) = slot.with_link(link, |l| radio.tally.snapshot(l.counters()))
        {
            *counters = own;
        }
        let tentative = radio.packed.tentative();
        // No await from here until the reply is queued: nothing else can
        // write the buffer in between.
        let len = match serialize_server_payload(&msg, radio.packed.table_for(&msg.msg)) {
            Ok(len) => len,
            Err(error) => {
                if switch_to.is_some() {
                    radio.packed.answer_dropped();
                }
                return Err(error);
            }
        };
        let queued = slot
            .with_link(link, |l| queue_reply(l, session, len))
            .unwrap_or(Queued::NoSession);
        match queued {
            Queued::Yes => {
                slot.ring();
                if is_hello {
                    radio.hello_owed = false;
                }
                if let Some(encoding) = switch_to {
                    radio.packed.answered(encoding);
                    log::info!("radio link {link}: replies are now {}", encoding.as_str());
                }
                Ok(())
            }
            Queued::NoSession => {
                radio.packed.rolled_back(tentative);
                if switch_to.is_some() {
                    radio.packed.answer_dropped();
                }
                log::debug!("radio link {link}: session ended; frame id={id} dropped");
                Ok(())
            }
            Queued::Refused(error) => {
                radio.packed.rolled_back(tentative);
                if switch_to.is_some() {
                    radio.packed.answer_dropped();
                }
                log::error!(
                    "radio link {link}: dropping frame id={id} ({len} B): refused ({error:?}) \
                     — closing"
                );
                self.close_radio(link, "write failed");
                Ok(())
            }
        }
    }
}

/// Queue the `len` bytes just serialized into the frame buffer on `link`, for
/// `session` only: copied into the send ring when short, else as an external
/// message the link reads out of the frame buffer.
fn queue_reply(
    link: &mut lp_link::Link<lp_link::SelectiveRepeat>,
    session: u32,
    len: usize,
) -> Queued {
    if link.state() != LinkState::Established || link.generation() != session {
        return Queued::NoSession;
    }
    if len <= SMALL_REPLY_BYTES && link.send(CH_PROTO, frame_bytes(len)).is_ok() {
        return Queued::Yes;
    }
    match link.send_external(CH_PROTO, len) {
        Ok(()) => Queued::Yes,
        Err(error) => Queued::Refused(error),
    }
}

/// Take a keyed link's handshake events into `out`: a key lookup for the
/// server (its key id kept for the answer), or a wrong key to charge to the
/// backoff. An initiator's events (`Refused`, `PeerNotSecure`) never reach a
/// responder.
#[cfg(feature = "wifi")]
fn take_secure_events(
    slot: &RadioLinkSlot,
    radio: &mut RadioLink,
    out: &mut Vec<(LinkId, SecureLinkEvent)>,
) {
    use lp_link::secure_channel::SecureEvent;
    while let Some(Some(event)) = slot.with_link(radio.id, |l| l.poll_secure_event()) {
        match event {
            SecureEvent::KeyLookup { key_id } => {
                radio.pending_key = Some(key_id);
                out.push((radio.id, SecureLinkEvent::KeyLookup { salt: key_id.0 }));
            }
            SecureEvent::WrongKey { key_id } => {
                out.push((radio.id, SecureLinkEvent::WrongKey { salt: key_id.0 }));
            }
            SecureEvent::Refused { .. } | SecureEvent::PeerNotSecure => {}
        }
    }
}

/// Whether a link's channel 3 goes to the update hook (rule 5): a
/// Bluetooth link's and a LAN link's, never a relayed one's yet.
fn carries_updates(wire: Link) -> bool {
    wire.trust != lpc_shared::transport::LinkTrust::Relayed
}

/// The end of a tick's wait budget, for a tick starting now.
fn tick_wait_until() -> Micros {
    now_us() + u64::from(TICK_WAIT_LIMIT_MS) * 1_000
}

/// How long the link in slot `index` (trusted as `trust`) may hold the
/// frame buffer: Bluetooth's air is slow ([`RADIO_WRITE_DEADLINE_MS`]), the
/// LAN's is not ([`LAN_WRITE_DEADLINE_MS`]), and the relay crosses the
/// internet (the slow path's bound again).
fn write_deadline_ms(index: usize, trust: Option<lpc_shared::transport::LinkTrust>) -> u32 {
    #[cfg(feature = "wifi")]
    if index >= RADIO_LINK_SLOTS && trust != Some(LinkTrust::Relayed) {
        return LAN_WRITE_DEADLINE_MS;
    }
    let _ = (index, trust);
    RADIO_WRITE_DEADLINE_MS
}

/// Resolves once `slot`'s link no longer reads the frame buffer, waking the
/// radio side to keep cutting frames meanwhile.
async fn wait_for_release(slot: &RadioLinkSlot) {
    while slot.external_in_flight() {
        slot.ring();
        slot.released().await;
    }
}

/// The device clock, in lp-link's unit: the one the radio side feeds the
/// links with.
pub fn now_us() -> Micros {
    embassy_time::Instant::now().as_micros()
}

impl<U: ServerTransport + FrameBufHolder, D: DelayNs> ServerTransport for LinkMuxTransport<U, D> {
    async fn send(&mut self, link: LinkId, msg: WireServerMessage) -> Result<(), TransportError> {
        if link == LinkId::PRIMARY {
            // The USB link serializes into the frame buffer too (rule 1).
            self.release_radio_holders().await;
            self.primary.send(link, msg).await
        } else {
            self.send_radio(link, msg).await
        }
    }

    async fn receive(&mut self) -> Result<Option<Incoming>, TransportError> {
        if let Some(incoming) = self.primary.receive().await? {
            return Ok(Some(incoming));
        }
        // Events before messages: a link's Opened always precedes its
        // messages, and its Closed always follows them.
        self.drain_events();
        self.pump_radio();
        Ok(self.inbox.pop_front())
    }

    async fn receive_all(&mut self) -> Result<Vec<Incoming>, TransportError> {
        let mut messages = Vec::new();
        while let Some(msg) = self.receive().await? {
            messages.push(msg);
        }
        Ok(messages)
    }

    fn links(&self) -> Vec<Link> {
        let mut links = self.primary.links();
        links.extend(self.radio.iter().map(|l| l.wire));
        links
    }

    fn take_closed_links(&mut self) -> Vec<LinkId> {
        let mut closed = self.primary.take_closed_links();
        closed.append(&mut self.closed);
        closed
    }

    #[cfg(feature = "wifi")]
    fn take_secure_events(&mut self) -> Vec<(LinkId, SecureLinkEvent)> {
        self.drain_events();
        self.pump_radio();
        // Drained, not taken: the list keeps its reserve (`new`).
        if self.secure.is_empty() {
            Vec::new()
        } else {
            self.secure.drain(..).collect()
        }
    }

    #[cfg(feature = "wifi")]
    fn answer_key_lookup(&mut self, link: LinkId, answer: KeyAnswer) {
        if self.challenge.as_ref().is_some_and(|c| c.id == link) {
            self.decide_challenge(answer);
            return;
        }
        let Some(radio) = self.radio.iter_mut().find(|l| l.id == link) else {
            return;
        };
        let Some(key_id) = radio.pending_key.take() else {
            return;
        };
        let slot = self.port.slot(radio.slot);
        slot.with_link(link, |l| answer_key_lookup(l, key_id, answer));
        slot.ring();
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        let ids: Vec<_> = self.radio.iter().map(|l| l.id).collect();
        for id in ids {
            self.close_radio(id, "transport closed");
        }
        self.primary.close().await
    }
}

impl<U: ServerTransport + FrameBufHolder + LinkUpkeep, D: DelayNs> LinkUpkeep
    for LinkMuxTransport<U, D>
{
    /// The primary's owed hello first (the USB link owes one on every
    /// session `Up`), then each radio link's whose session came up (rule 3).
    fn take_opened_links(&mut self) -> Vec<Link> {
        let mut opened = self.primary.take_opened_links();
        self.drain_events();
        self.pump_radio();
        for radio in &mut self.radio {
            // A keyed link's hello waits for its grant: the server takes the
            // handshake's `Authenticated` in its next tick, and a hello built
            // before that would say the link holds nothing (found by the
            // host LAN harness, P06).
            #[cfg(feature = "wifi")]
            if self.secure.iter().any(|(id, event)| {
                *id == radio.id && matches!(event, SecureLinkEvent::Authenticated { .. })
            }) {
                continue;
            }
            if core::mem::take(&mut radio.hello_owed) {
                opened.push(radio.wire);
            }
        }
        opened
    }

    fn upkeep(&mut self, server: &LpServer, now_ms: u64) {
        // The end of a tick: the next one's wait budget starts here.
        self.tick_wait_until = tick_wait_until();
        self.primary.upkeep(server, now_ms);
        self.expire_unauthenticated(
            now_ms,
            |link| server.link_tier(link).is_some(),
            |link| server.login_pending(link),
        );
        self.dispatch_updates(
            |link| server.link_granted_tier(link),
            || server.device_open(),
        );
        if let Some(hook) = self.upkeep_hook {
            hook(server, now_ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use alloc::rc::Rc;
    use alloc::string::ToString;
    use alloc::vec;
    use lp_link::{LinkConfig, SelectiveRepeat};
    use lpc_shared::transport::LinkTrust;
    #[cfg(any(feature = "json-pack", feature = "wifi"))]
    use lpc_wire::WireEncoding;

    extern crate std;

    use super::super::{OpenRefused, RadioLinkMode, UPDATE_RX_WINDOW};
    use crate::serial::server_msg::frame_buf_turn;
    use crate::update_send::UpdateSend;

    /// The whole life of a radio link: announced at subscribe, its lp-link
    /// session comes up, its hello goes first, then requests and replies,
    /// then it closes.
    #[test]
    fn a_radio_link_owes_its_hello_on_up_then_carries_requests_and_replies() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        let mut central = Central::new(0x0bad_cafe, 247);
        let link = open_link(port, &mut mux, 0, 247);
        assert_ne!(link, LinkId::PRIMARY);
        assert_eq!(mux.links().len(), 2);
        assert!(
            mux.take_opened_links().is_empty(),
            "subscribed, but no session yet: no hello"
        );

        // The handshake is ordinary traffic; the hello is owed at `Up`.
        central.pump(port.slot(0));
        assert!(central.saw_up());
        // A request the host sends at once waits behind the hello.
        central.send_request(&hello_request(7));
        central.pump(port.slot(0));
        assert!(
            block(mux.receive()).unwrap().is_none(),
            "not before the hello"
        );
        assert_eq!(mux.take_opened_links(), vec![RadioLinkPort::link(link)]);
        assert!(mux.take_opened_links().is_empty(), "owed once per Up");
        block(mux.send(link, hello())).unwrap();
        central.pump(port.slot(0));
        let first = central.next_proto().expect("the hello");
        assert_eq!(first[0], b'{', "JSON until the host opts in");
        let first: WireServerMessage = lpc_wire::json::from_slice(&first).unwrap();
        assert!(matches!(first.msg, ServerMsgBody::Hello(_)));

        let incoming = block(mux.receive()).unwrap().expect("the request");
        assert_eq!(incoming.link, link);
        assert_eq!(incoming.trust, LinkTrust::Untrusted);
        assert_eq!(incoming.msg.id, 7);
        assert!(block(mux.receive()).unwrap().is_none());

        block(mux.send(link, error_reply(7, 20))).unwrap();
        assert!(
            !port.slot(0).external_in_flight(),
            "a short reply is copied into the link, not held in the frame buffer"
        );
        central.pump(port.slot(0));
        let reply: WireServerMessage =
            lpc_wire::json::from_slice(&central.next_proto().unwrap()).unwrap();
        assert_eq!(reply.id, 7);

        port.slot(0).close();
        block(port.announce(RadioLinkEvent::Closed { link }));
        assert!(block(mux.receive()).unwrap().is_none());
        assert_eq!(mux.take_closed_links(), vec![link]);
        assert_eq!(mux.links(), vec![Link::PRIMARY]);
    }

    /// A long reply is read out of the frame buffer by its radio link, a
    /// fragment at a time; the USB link's next reply waits for that and does
    /// not write the buffer under it.
    #[test]
    fn a_long_reply_is_read_from_the_frame_buffer_and_usb_waits_for_it() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let usb = Usb {
            port: Some(port),
            ..Usb::default()
        };
        let mut mux = LinkMuxTransport::new(usb, port, NeverDelay);
        let mut central = Central::new(0x1111_2222, 247);
        let link = open_session(port, &mut mux, &mut central, 0, 247);

        block(mux.send(link, error_reply(21, 9_000))).unwrap();
        assert!(port.slot(0).external_in_flight(), "the link is reading it");
        let (sent, ()) = block(embassy_futures::join::join(
            mux.send(LinkId::PRIMARY, error_reply(22, 10)),
            async {
                while port.slot(0).external_in_flight() {
                    embassy_time::Timer::after_millis(1).await;
                    central.pump(port.slot(0));
                }
            },
        ));
        sent.unwrap();
        assert_eq!(mux.primary.sent, vec![22]);
        assert_eq!(
            mux.primary.sent_while_held, 0,
            "USB serialized only once the radio link let go"
        );
        central.pump(port.slot(0));
        let big: WireServerMessage =
            lpc_wire::json::from_slice(&central.next_proto().unwrap()).unwrap();
        assert_eq!(big.id, 21);
        assert_eq!(central.max_frame, 188, "every frame one 247-MTU value");
    }

    /// A radio link that does not take its long reply out of the frame buffer
    /// in time is closed, and gives the buffer up at once.
    #[test]
    fn a_radio_link_that_holds_the_frame_buffer_past_its_deadline_is_closed() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let usb = Usb {
            port: Some(port),
            ..Usb::default()
        };
        let mut mux = LinkMuxTransport::new(usb, port, ReadyDelay);
        let mut central = Central::new(0x3333_4444, 247);
        let link = open_session(port, &mut mux, &mut central, 1, 247);
        block(mux.send(link, error_reply(31, 9_000))).unwrap();
        assert!(port.slot(1).external_in_flight());
        // The central stops reading; the (immediately elapsed) deadline wins.
        block(mux.send(LinkId::PRIMARY, error_reply(32, 10))).unwrap();
        assert_eq!(mux.primary.sent_while_held, 0);
        assert!(!port.slot(1).external_in_flight(), "dropped: buffer free");
        assert_eq!(mux.take_closed_links(), vec![link]);
        assert_eq!(port.slot(1).take_close_request(), Some("reply deadline"));
        // Frames still addressed to it are skipped, not errors.
        assert!(block(mux.send(link, error_reply(33, 10))).is_ok());
    }

    /// A tick that has already spent its wait budget (a project load, a
    /// compile) does not wait on a stalled link at all: the link is closed
    /// at once and the tick goes on, so a load and a stalled peer can never
    /// add up past the watchdog (PR C's walk: 12 resets).
    #[test]
    fn a_tick_past_its_wait_budget_closes_a_stalled_link_without_waiting() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let usb = Usb {
            port: Some(port),
            ..Usb::default()
        };
        let waited = Rc::new(core::cell::Cell::new(0u64));
        let mut mux = LinkMuxTransport::new(usb, port, CountingDelay(Rc::clone(&waited)));
        let mut central = Central::new(0x3333_5555, 247);
        let link = open_session(port, &mut mux, &mut central, 1, 247);
        block(mux.send(link, error_reply(41, 9_000))).unwrap();
        assert!(port.slot(1).external_in_flight());
        // The tick began long enough ago that its budget is spent.
        mux.tick_wait_until = now_us();
        block(mux.send(LinkId::PRIMARY, error_reply(42, 10))).unwrap();
        assert_eq!(waited.get(), 0, "no wait at all");
        assert_eq!(mux.take_closed_links(), vec![link]);
        assert_eq!(port.slot(1).take_close_request(), Some("reply deadline"));
    }

    /// A stalled peer's wait is its link's deadline, never more: a whole
    /// Bluetooth deadline, a LAN link's second.
    #[test]
    fn a_stalled_links_wait_is_its_own_deadline() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let usb = Usb {
            port: Some(port),
            ..Usb::default()
        };
        let waited = Rc::new(core::cell::Cell::new(0u64));
        let mut mux = LinkMuxTransport::new(usb, port, CountingDelay(Rc::clone(&waited)));
        let mut central = Central::new(0x3333_6666, 247);
        let link = open_session(port, &mut mux, &mut central, 1, 247);
        block(mux.send(link, error_reply(43, 9_000))).unwrap();
        // A fresh tick with budget to spare: the link's deadline decides.
        mux.tick_wait_until = now_us() + 10_000_000;
        block(mux.send(LinkId::PRIMARY, error_reply(44, 10))).unwrap();
        assert_eq!(waited.get(), u64::from(RADIO_WRITE_DEADLINE_MS) * 1_000_000);
        assert_eq!(mux.take_closed_links(), vec![link]);
        assert_eq!(write_deadline_ms(0, None), RADIO_WRITE_DEADLINE_MS);
        #[cfg(feature = "wifi")]
        {
            use lpc_shared::transport::LinkTrust;
            assert_eq!(
                write_deadline_ms(RADIO_LINK_SLOTS, Some(LinkTrust::Keyed)),
                LAN_WRITE_DEADLINE_MS
            );
            assert_eq!(
                write_deadline_ms(RADIO_LINK_SLOTS, Some(LinkTrust::Relayed)),
                RADIO_WRITE_DEADLINE_MS,
                "the relay crosses the internet: the slow path's bound"
            );
        }
    }

    /// Packing exists only with `json-pack`; without it every reply is JSON.
    #[cfg(feature = "json-pack")]
    #[test]
    fn a_packed_session_codes_against_a_table_and_a_new_session_is_json_again() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        let mut central = Central::new(0x5555_6666, 247);
        let link = open_session(port, &mut mux, &mut central, 0, 247);

        block(mux.send(link, set_encoding(WireEncoding::Packed))).unwrap();
        central.pump(port.slot(0));
        assert_eq!(central.next_proto().unwrap()[0], b'{', "the answer is JSON");
        let mut table = lp_json_pack::LearnedTable::NEW;
        for n in 0..5 {
            block(mux.send(link, log_reply(n))).unwrap();
            central.pump(port.slot(0));
            let payload = central.next_proto().unwrap();
            assert_eq!(payload[0], b'L', "reply {n}");
            let mut json = Vec::new();
            lp_json_pack::decode_learned(
                &lp_json_pack::Dictionary::EMPTY,
                &mut table,
                &payload[1..],
                &mut json,
            )
            .unwrap_or_else(|e| panic!("reply {n}: {e:?}"));
            let decoded: WireServerMessage = lpc_wire::json::from_slice(&json).unwrap();
            assert_eq!(decoded.id, n);
        }

        // The page reloads over the same connection: a new host session. The
        // board resets with it, owes a new hello, and is JSON again.
        let mut central = Central::new(0x7777_8888, 247);
        central.pump(port.slot(0));
        assert!(central.saw_up());
        assert!(block(mux.receive()).unwrap().is_none());
        assert_eq!(mux.take_opened_links(), vec![RadioLinkPort::link(link)]);
        block(mux.send(link, hello())).unwrap();
        block(mux.send(link, log_reply(9))).unwrap();
        central.pump(port.slot(0));
        assert_eq!(central.next_proto().unwrap()[0], b'{', "the hello");
        assert_eq!(central.next_proto().unwrap()[0], b'{', "JSON again");
        assert!(mux.take_closed_links().is_empty(), "same link, same tier");
    }

    /// Two centrals, two slots: each its own lp-link session, its own hello,
    /// its own link id on every request.
    #[test]
    fn each_central_gets_its_own_session() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        let mut a = Central::new(0x0a0a_0a0a, 247);
        let mut b = Central::new(0x0b0b_0b0b, 185);
        let link_a = open_link(port, &mut mux, 0, 247);
        let link_b = open_link(port, &mut mux, 1, 185);
        a.pump(port.slot(0));
        b.pump(port.slot(1));
        assert_eq!(
            mux.take_opened_links(),
            vec![RadioLinkPort::link(link_a), RadioLinkPort::link(link_b)]
        );
        block(mux.send(link_a, hello())).unwrap();
        block(mux.send(link_b, hello())).unwrap();
        a.send_request(&hello_request(1));
        b.send_request(&hello_request(2));
        a.pump(port.slot(0));
        b.pump(port.slot(1));
        assert!(a.next_proto().is_some());
        assert!(b.next_proto().is_some());
        let mut got: Vec<_> = [block(mux.receive()), block(mux.receive())]
            .into_iter()
            .map(|r| r.unwrap().unwrap())
            .map(|i| (i.link, i.msg.id))
            .collect();
        got.sort_by_key(|(_, id)| *id);
        assert_eq!(got, vec![(link_a, 1), (link_b, 2)]);

        // A long reply to the iOS-sized central: every frame fits its 182 B
        // values, and it arrives whole.
        block(mux.send(link_b, error_reply(3, 4_000))).unwrap();
        while port.slot(1).external_in_flight() {
            b.pump(port.slot(1));
        }
        b.pump(port.slot(1));
        let reply: WireServerMessage =
            lpc_wire::json::from_slice(&b.next_proto().unwrap()).unwrap();
        assert_eq!(reply.id, 3);
        assert_eq!(b.max_frame, 182, "MTU 185: 182-byte values, filled");
    }

    /// The server fills a heartbeat's counters from the USB link; a radio
    /// link's own go out on it instead — here, the bad request it counted.
    #[test]
    fn a_radio_links_heartbeat_carries_its_own_counters() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        let mut central = Central::new(0x2468_1357, 247);
        let link = open_session(port, &mut mux, &mut central, 0, 247);
        central.link.send(CH_PROTO, b"M!{not json").unwrap();
        central.pump(port.slot(0));
        assert!(block(mux.receive()).unwrap().is_none(), "dropped");

        let counters = Some(lpc_wire::server::LinkCounters::default());
        block(mux.send(link, heartbeat(counters))).unwrap();
        central.pump(port.slot(0));
        let beat: WireServerMessage =
            lpc_wire::json::from_slice(&central.next_proto().unwrap()).unwrap();
        let ServerMsgBody::Heartbeat {
            link: Some(counters),
            ..
        } = beat.msg
        else {
            panic!("a heartbeat with counters");
        };
        assert_eq!(counters.payload_errors, 1);
        assert_eq!(counters.ups, 1);
        assert!(counters.frames_rx > 0);

        // A link with no tier gets none: the server's `None` is kept.
        block(mux.send(link, heartbeat(None))).unwrap();
        central.pump(port.slot(0));
        let beat: WireServerMessage =
            lpc_wire::json::from_slice(&central.next_proto().unwrap()).unwrap();
        assert!(matches!(
            beat.msg,
            ServerMsgBody::Heartbeat { link: None, .. }
        ));
    }

    /// Both radio slots occupied at the board's own config (D10): what the two
    /// links hold at rest, after request/reply traffic, at the peak of a
    /// large upload on each, and once that is released. `--nocapture` prints
    /// the figures (host, 64-bit; the C6 is 32-bit, where the descriptors
    /// and event queue are about half).
    #[test]
    fn two_open_radio_links_cost_this_much_ram() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        let mut a = Central::new(0x1111_1111, 247);
        let mut b = Central::new(0x3333_3333, 247);
        let link_a = open_session(port, &mut mux, &mut a, 0, 247);
        let link_b = open_session(port, &mut mux, &mut b, 1, 247);
        let ram = |slot: usize| port.slot(slot).ram_bytes().unwrap();
        let at_rest = (ram(0), ram(1));

        for n in 0..3 {
            for (link, central, slot) in [(link_a, &mut a, 0), (link_b, &mut b, 1)] {
                central.send_request(&hello_request(n));
                central.pump(port.slot(slot));
                assert!(block(mux.receive()).unwrap().is_some());
                block(mux.send(link, error_reply(n, 2_000))).unwrap();
                central.pump(port.slot(slot));
                assert!(central.next_proto().is_some());
            }
        }
        let traffic = (ram(0), ram(1));

        // A large upload on each (the payload does not have to decode).
        let mut upload = vec![b'{'];
        upload.resize(6 * 1024, b' ');
        for (central, slot) in [(&mut a, 0), (&mut b, 1)] {
            central.peak_ram = 0;
            central.link.send(CH_PROTO, &upload).unwrap();
            central.pump(port.slot(slot));
        }
        let peak = (a.peak_ram, b.peak_ram);
        assert!(block(mux.receive()).unwrap().is_none(), "counted, dropped");
        let released = (ram(0), ram(1));

        let cfg = super::super::radio_link_config(247, RadioLinkMode::Serve).unwrap();
        let bound = lp_link::Link::<SelectiveRepeat>::ram_bound(&cfg);
        let pair = |(x, y): (usize, usize)| alloc::format!("{x} + {y} = {} B", x + y);
        std::println!(
            "two radio links (max_payload {} B, send_budget {} B, keep_reassembly {} B):",
            cfg.max_payload,
            cfg.send_budget,
            cfg.keep_reassembly
        );
        std::println!("  at rest (session up, hello sent): {}", pair(at_rest));
        std::println!("  after 3 request/reply rounds each: {}", pair(traffic));
        std::println!("  peak during a 6 KiB upload on each: {}", pair(peak));
        std::println!("  after the upload is delivered: {}", pair(released));
        std::println!(
            "  worst case (ram_bound): {bound} B each, {} B both",
            2 * bound
        );
        assert!(peak.0 <= bound && peak.1 <= bound);
        assert!(
            released.0 + released.1 <= at_rest.0 + at_rest.1 + 512,
            "the upload's reassembly buffer is given back"
        );
    }

    /// A LAN link never packs: its opt-in is answered `json`, so no learned
    /// table is allocated for it.
    #[cfg(feature = "wifi")]
    #[test]
    fn a_lan_links_packed_opt_in_is_answered_json() {
        let mut answer = ServerMsgBody::SetEncoding {
            encoding: WireEncoding::Packed,
        };
        stay_json(&mut answer);
        assert!(matches!(
            answer,
            ServerMsgBody::SetEncoding {
                encoding: WireEncoding::Json
            }
        ));
    }

    #[test]
    fn an_unauthenticated_link_is_closed_after_the_deadline() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        // A login runs over a session that is up.
        let mut central = Central::new(0x1357_2468, 247);
        let link = open_session(port, &mut mux, &mut central, 0, 247);
        mux.expire_unauthenticated(1_000, |_| false, |_| false);
        mux.expire_unauthenticated(1_000 + LOGIN_DEADLINE_MS - 1, |_| false, |_| false);
        assert!(mux.take_closed_links().is_empty());
        mux.expire_unauthenticated(1_000 + LOGIN_DEADLINE_MS, |_| false, |_| false);
        assert_eq!(mux.take_closed_links(), vec![link]);
        assert_eq!(
            port.slot(0).take_close_request(),
            Some("no login within the deadline")
        );
        assert!(
            port.slot(0).poll_timeout().is_none(),
            "its link is dropped at once, before the radio disconnects"
        );
    }

    #[test]
    fn a_link_that_logs_in_is_never_expired() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        let mut central = Central::new(0x2468_1357, 247);
        let _link = open_session(port, &mut mux, &mut central, 0, 247);
        mux.expire_unauthenticated(0, |_| false, |_| false);
        mux.expire_unauthenticated(5_000, |_| true, |_| false);
        mux.expire_unauthenticated(60_000, |_| false, |_| false);
        assert!(mux.take_closed_links().is_empty());
    }

    /// A link whose session never comes up is closed at the deadline even
    /// on an open board, where it would hold a tier from the start: it
    /// speaks nothing and only holds the slot.
    #[test]
    fn a_link_whose_session_never_comes_up_is_closed_even_on_an_open_board() {
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        let link = open_link(port, &mut mux, 0, 247);
        mux.expire_unauthenticated(0, |_| true, |_| false);
        mux.expire_unauthenticated(LOGIN_DEADLINE_MS - 1, |_| true, |_| false);
        assert!(
            mux.take_closed_links().is_empty(),
            "not before the deadline"
        );
        mux.expire_unauthenticated(LOGIN_DEADLINE_MS, |_| true, |_| false);
        assert_eq!(mux.take_closed_links(), vec![link]);
        assert_eq!(
            port.slot(0).take_close_request(),
            Some("no session within the deadline")
        );
    }

    #[test]
    fn an_outstanding_login_holds_the_deadline_until_it_ends() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        // A login runs over a session that is up.
        let mut central = Central::new(0x1357_2468, 247);
        let link = open_session(port, &mut mux, &mut central, 0, 247);
        mux.expire_unauthenticated(1_000, |_| false, |_| false);
        // LoginBegin at 9 s; the person is still typing at 10 s and at 38 s.
        mux.expire_unauthenticated(1_000 + LOGIN_DEADLINE_MS, |_| false, |_| true);
        mux.expire_unauthenticated(38_000, |_| false, |_| true);
        assert!(mux.take_closed_links().is_empty());
        // The challenge expired (the server says so): the link is past its
        // own deadline, so it closes on the next upkeep.
        mux.expire_unauthenticated(39_000, |_| false, |_| false);
        assert_eq!(mux.take_closed_links(), vec![link]);
        assert_eq!(
            port.slot(0).take_close_request(),
            Some("no login within the deadline")
        );
    }

    #[test]
    fn a_login_begun_before_the_deadline_and_refused_does_not_extend_it() {
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        let link = open_link(port, &mut mux, 0, 247);
        mux.expire_unauthenticated(0, |_| false, |_| false);
        // Pending at 4 s; refused at 6 s (no longer pending): the ordinary
        // 10 s deadline still holds and nothing moved it.
        mux.expire_unauthenticated(4_000, |_| false, |_| true);
        mux.expire_unauthenticated(6_000, |_| false, |_| false);
        mux.expire_unauthenticated(LOGIN_DEADLINE_MS - 1, |_| false, |_| false);
        assert!(mux.take_closed_links().is_empty());
        mux.expire_unauthenticated(LOGIN_DEADLINE_MS, |_| false, |_| false);
        assert_eq!(mux.take_closed_links(), vec![link]);
    }

    /// The USB link owes a hello on every session `Up`; behind the mux the
    /// server loop must still hear about it, before any radio link's.
    #[test]
    fn the_primarys_owed_hello_is_passed_on_first() {
        let port = leak_port();
        let usb = Usb {
            opened: vec![Link::PRIMARY],
            ..Usb::default()
        };
        let mut mux = LinkMuxTransport::new(usb, port, NeverDelay);
        let mut central = Central::new(0x1357_2468, 247);
        let link = open_link(port, &mut mux, 0, 247);
        central.pump(port.slot(0));
        assert_eq!(
            mux.take_opened_links(),
            vec![Link::PRIMARY, RadioLinkPort::link(link)]
        );
        assert!(mux.take_opened_links().is_empty(), "owed once");
    }

    #[test]
    fn usb_traffic_passes_through_untouched() {
        let port = leak_port();
        let mut usb = Usb::default();
        usb.inbox.push(Incoming::primary(hello_request(9)));
        let mut mux = LinkMuxTransport::new(usb, port, NeverDelay);
        let incoming = block(mux.receive()).unwrap().unwrap();
        assert_eq!(incoming.link, LinkId::PRIMARY);
        block(mux.send(LinkId::PRIMARY, error_reply(9, 10))).unwrap();
        assert_eq!(mux.primary.sent, vec![9]);
    }

    /// No link opens before the boot decides what its radio links are for:
    /// the radio side waits for the decision, and a decision made is final.
    #[test]
    fn no_link_opens_before_the_mode_is_decided() {
        let port: &'static RadioLinkPort = Box::leak(Box::new(RadioLinkPort::new()));
        let link = port.mint_link();
        assert_eq!(port.mode(), None);
        assert_eq!(
            port.open(0, link, 247, 1),
            Err(OpenRefused::ModeUndecided),
            "the SYN would carry a window nobody chose"
        );
        assert!(port.slot(0).poll_timeout().is_none(), "nothing opened");
        let mut waiting = core::pin::pin!(port.wait_for_mode(0));
        assert!(
            embassy_futures::poll_once(waiting.as_mut()).is_pending(),
            "a connection waits for the decision"
        );

        port.decide_mode(RadioLinkMode::Update);
        assert_eq!(
            embassy_futures::poll_once(waiting.as_mut()),
            core::task::Poll::Ready(RadioLinkMode::Update),
            "the decision wakes it"
        );
        assert_eq!(block(port.wait_for_mode(1)), RadioLinkMode::Update);
        assert_eq!(port.open(0, link, 247, 1), Ok(180));

        // A second decision cannot reconfigure what is already open.
        port.decide_mode(RadioLinkMode::Serve);
        assert_eq!(port.mode(), Some(RadioLinkMode::Update));
    }

    /// The SYN — a link's first frame — advertises the wide receive window
    /// in update mode (core-only) and the preset's in serve mode.
    #[test]
    fn a_link_opened_in_update_mode_advertises_the_wide_window_in_its_syn() {
        for (mode, window) in [
            (RadioLinkMode::Update, UPDATE_RX_WINDOW),
            (RadioLinkMode::Serve, LinkConfig::ble().rx_window),
        ] {
            let port = leak_port_in(mode);
            port.open(0, port.mint_link(), 247, 7).unwrap();
            let syn = port
                .slot(0)
                .poll_frame(0, <[u8]>::to_vec)
                .expect("a SYN first");
            let header = lp_link::frame::Header::parse(&syn).unwrap();
            assert_eq!(header.kind, lp_link::frame::FrameKind::Syn);
            let body = &syn[lp_link::frame::HEADER_LEN..syn.len() - 4];
            let syn = lp_link::frame::SynBody::parse(body).unwrap();
            assert_eq!(syn.rx_window, window, "{mode:?}");
            assert_eq!(syn.max_payload, 180, "{mode:?}");
        }
        assert_eq!(UPDATE_RX_WINDOW, 32);
    }

    /// Rule 5: a radio link's channel-3 message goes to the core's update
    /// hook from the upkeep, with the tier its login granted — not with the
    /// wire's requests; a closed link is passed on and its queued messages
    /// dropped; a pass follows every dispatch.
    #[test]
    fn channel_three_goes_to_the_update_hook_with_the_granted_tier() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let mut mux =
            LinkMuxTransport::new(Usb::default(), port, NeverDelay).with_update_hook(record_update);
        let mut a = Central::new(0x0a0a_1111, 247);
        let mut b = Central::new(0x0b0b_2222, 247);
        let link_a = open_session(port, &mut mux, &mut a, 0, 247);
        let link_b = open_session(port, &mut mux, &mut b, 1, 247);
        take_updates();

        a.link.send(CH_UPDATE, b"Q\x01").unwrap();
        a.send_request(&hello_request(4));
        b.link.send(CH_UPDATE, b"G\x01").unwrap();
        a.pump(port.slot(0));
        b.pump(port.slot(1));
        let incoming = block(mux.receive()).unwrap().expect("the wire request");
        assert_eq!((incoming.link, incoming.msg.id), (link_a, 4));
        assert!(
            block(mux.receive()).unwrap().is_none(),
            "channel 3 is not a request"
        );
        assert!(take_updates().is_empty(), "nothing before the upkeep");

        // The server granted A play by a login; B holds nothing. The
        // device's `open` setting as it is now rides along, for the session
        // to apply by its own rule (it was locked since boot, say).
        mux.dispatch_updates(|l| (l.id == link_a).then_some(Tier::Play), || OpenTo::Play);
        assert_eq!(
            take_updates(),
            vec![
                Seen::Message(link_a, Some(Tier::Play), OpenTo::Play, b"Q\x01".to_vec()),
                Seen::Message(link_b, None, OpenTo::Play, b"G\x01".to_vec()),
                Seen::Pass,
            ]
        );
        mux.dispatch_updates(|_| None, || OpenTo::Nobody);
        assert_eq!(take_updates(), vec![Seen::Pass], "a pass every upkeep");

        // B sends again and closes before the upkeep: the hook hears it
        // closed, never its message.
        b.link.send(CH_UPDATE, b"Q\x01").unwrap();
        b.pump(port.slot(1));
        assert!(block(mux.receive()).unwrap().is_none());
        port.slot(1).close();
        block(port.announce(RadioLinkEvent::Closed { link: link_b }));
        assert!(block(mux.receive()).unwrap().is_none());
        mux.dispatch_updates(|_| Some(Tier::Edit), || OpenTo::Nobody);
        assert_eq!(take_updates(), vec![Seen::Closed(link_b), Seen::Pass]);

        // A link the mux closes itself is passed on the same way.
        mux.expire_unauthenticated(0, |_| false, |_| false);
        mux.expire_unauthenticated(LOGIN_DEADLINE_MS, |_| false, |_| false);
        mux.dispatch_updates(|_| None, || OpenTo::Nobody);
        assert_eq!(take_updates(), vec![Seen::Closed(link_a), Seen::Pass]);
    }

    /// A monolithic image installs no hook: channel 3 is ignored, never
    /// queued.
    #[test]
    fn without_an_update_hook_channel_three_is_ignored() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        let mut central = Central::new(0x0c0c_3333, 247);
        let _link = open_session(port, &mut mux, &mut central, 0, 247);
        central.link.send(CH_UPDATE, b"Q\x01").unwrap();
        central.pump(port.slot(0));
        assert!(block(mux.receive()).unwrap().is_none());
        assert!(mux.updates.is_empty());
        mux.dispatch_updates(|_| None, || OpenTo::Nobody);
    }

    /// Rule 5 on the LAN: a LAN link's channel-3 message goes to the hook
    /// with the tier its key granted it on the server (asked with the link
    /// as the server knows it: keyed), and a relayed link's is ignored.
    #[cfg(feature = "wifi")]
    #[test]
    fn a_lan_links_channel_three_goes_to_the_update_hook_with_its_keys_grant() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let mut mux =
            LinkMuxTransport::new(Usb::default(), port, NeverDelay).with_update_hook(record_update);
        take_updates();

        let (lan, mut client) = open_network_session(port, &mut mux, LinkTrust::Keyed);
        client.link.send(CH_UPDATE, b"Q\x01").unwrap();
        client.pump(port.slot(RADIO_LINK_SLOTS));
        assert!(block(mux.receive()).unwrap().is_none(), "not a request");
        mux.dispatch_updates(
            |l| (l.id == lan && l.trust == LinkTrust::Keyed).then_some(Tier::Edit),
            || OpenTo::Nobody,
        );
        assert_eq!(
            take_updates(),
            vec![
                Seen::Message(lan, Some(Tier::Edit), OpenTo::Nobody, b"Q\x01".to_vec()),
                Seen::Pass,
            ]
        );
        // The board's answer goes back on the LAN link.
        assert_eq!(port.send_update(lan, b"M{}"), UpdateSend::Queued);
        client.pump(port.slot(RADIO_LINK_SLOTS));
        assert_eq!(client.next_update(), Some(b"M{}".to_vec()));
        port.slot(RADIO_LINK_SLOTS).close_link(lan);
        block(port.announce(RadioLinkEvent::Closed { link: lan }));
        assert!(block(mux.receive()).unwrap().is_none());
        mux.dispatch_updates(|_| None, || OpenTo::Nobody);
        assert_eq!(take_updates(), vec![Seen::Closed(lan), Seen::Pass]);

        let (relayed, mut client) = open_network_session(port, &mut mux, LinkTrust::Relayed);
        client.link.send(CH_UPDATE, b"Q\x01").unwrap();
        client.pump(port.slot(RADIO_LINK_SLOTS));
        assert!(block(mux.receive()).unwrap().is_none());
        mux.dispatch_updates(|_| Some(Tier::Edit), || OpenTo::Nobody);
        assert_eq!(take_updates(), vec![Seen::Pass], "a relayed link's is ignored");
        let _ = relayed;
    }

    /// The hook's answers on a radio link: a small one through the link's
    /// send ring, a 4 KiB read-back chunk as the external message out of the
    /// frame buffer, and another large one waits while any link reads it.
    #[test]
    fn update_answers_go_by_ring_or_by_frame_buffer_on_a_radio_link() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        let mut a = Central::new(0x0d0d_4444, 247);
        let mut b = Central::new(0x0e0e_5555, 247);
        let link_a = open_session(port, &mut mux, &mut a, 0, 247);
        let link_b = open_session(port, &mut mux, &mut b, 1, 247);
        let unknown = port.mint_link();
        assert_eq!(port.send_update(unknown, b"M{}"), UpdateSend::NoSession);

        let manifest = b"M{\"proto\":1,\"state\":\"running\"}";
        assert_eq!(port.send_update(link_a, manifest), UpdateSend::Queued);
        assert!(!port.frame_buf_in_use(), "a small answer holds nothing");
        let chunk: Vec<u8> = (0..4102u32).map(|i| i as u8).collect();
        assert_eq!(port.send_update(link_a, &chunk), UpdateSend::Queued);
        assert!(port.frame_buf_in_use());
        assert_eq!(
            port.send_update(link_b, &chunk),
            UpdateSend::Later,
            "another link's large answer waits for the buffer"
        );
        assert_eq!(port.send_update(link_b, manifest), UpdateSend::Queued);
        while port.frame_buf_in_use() {
            a.pump(port.slot(0));
        }
        a.pump(port.slot(0));
        b.pump(port.slot(1));
        assert_eq!(a.next_update(), Some(manifest.to_vec()));
        assert_eq!(a.next_update(), Some(chunk.clone()));
        assert_eq!(b.next_update(), Some(manifest.to_vec()));
        assert_eq!(port.send_update(link_b, &chunk), UpdateSend::Queued);
    }

    // ---- helpers ----

    /// A port whose boot decided [`RadioLinkMode::Serve`] (the engine runs).
    fn leak_port() -> &'static RadioLinkPort {
        leak_port_in(RadioLinkMode::Serve)
    }

    fn leak_port_in(mode: RadioLinkMode) -> &'static RadioLinkPort {
        let port: &'static RadioLinkPort = Box::leak(Box::new(RadioLinkPort::new()));
        port.decide_mode(mode);
        port
    }

    /// What the update hook was handed, owned.
    #[derive(Debug, PartialEq, Eq)]
    enum Seen {
        Message(LinkId, Option<Tier>, OpenTo, Vec<u8>),
        Closed(LinkId),
        Pass,
    }

    std::thread_local! {
        /// The hook is a plain `fn`, called on the test's own thread.
        static SEEN: core::cell::RefCell<Vec<Seen>> = const { core::cell::RefCell::new(Vec::new()) };
    }

    fn record_update(call: RadioUpdate<'_>) {
        let seen = match call {
            RadioUpdate::Message {
                link,
                granted,
                open,
                bytes,
            } => Seen::Message(link, granted, open, bytes.to_vec()),
            RadioUpdate::Closed { link } => Seen::Closed(link),
            RadioUpdate::Pass => Seen::Pass,
        };
        SEEN.with(|s| s.borrow_mut().push(seen));
    }

    fn take_updates() -> Vec<Seen> {
        SEEN.with(|s| core::mem::take(&mut *s.borrow_mut()))
    }

    /// What the BLE task does when a central subscribes: open the slot's
    /// lp-link session at the connection's MTU and announce it.
    fn open_link<D: DelayNs>(
        port: &'static RadioLinkPort,
        mux: &mut LinkMuxTransport<Usb, D>,
        slot: usize,
        att_mtu: u16,
    ) -> LinkId {
        let link = port.mint_link();
        port.slot(slot).reset();
        port.open(slot, link, att_mtu, 0x5eed_0000 + slot as u32)
            .unwrap();
        block(port.announce(RadioLinkEvent::Opened { link, slot }));
        assert!(block(mux.receive()).unwrap().is_none());
        link
    }

    /// What the LAN endpoint (or the relay driver) does with a new
    /// connection, then the secure handshake with an edit key the server
    /// knows, then the hello: the link and its client.
    #[cfg(feature = "wifi")]
    fn open_network_session<D: DelayNs>(
        port: &'static RadioLinkPort,
        mux: &mut LinkMuxTransport<Usb, D>,
        trust: LinkTrust,
    ) -> (LinkId, SecureClient) {
        use lp_link::secure_channel::{KeyId, Psk, SecureRole};
        let index = RADIO_LINK_SLOTS;
        let edge = if trust == LinkTrust::Relayed {
            super::super::SlotEdge::Relay
        } else {
            super::super::SlotEdge::Local
        };
        let link = port.mint_link();
        port.slot(index)
            .open_network(link, 0x5eed, fill, trust, edge)
            .unwrap();
        block(port.announce(RadioLinkEvent::Opened { link, slot: index }));
        assert!(block(mux.receive()).unwrap().is_none());
        let psk = [0x42; 32];
        let mut client = SecureClient {
            link: lp_link::Link::new_secure(
                LinkConfig::ws(),
                0x0c11_e470,
                SecureRole::Initiator {
                    key_id: KeyId([2; 16]),
                    psk: Psk::new(psk),
                },
                fill,
            ),
            events: VecDeque::new(),
            now: 1_000_000_000_000,
        };
        client.pump(port.slot(index));
        let events = mux.take_secure_events();
        assert_eq!(
            events,
            vec![(link, SecureLinkEvent::KeyLookup { salt: [2; 16] })]
        );
        mux.answer_key_lookup(link, KeyAnswer::Keys(vec![psk]));
        client.pump(port.slot(index));
        assert!(block(mux.receive()).unwrap().is_none());
        // The server takes the grant before the hello (its tick's order).
        assert_eq!(
            mux.take_secure_events(),
            vec![(
                link,
                SecureLinkEvent::Authenticated {
                    salt: [2; 16],
                    candidate: 0
                }
            )]
        );
        assert_eq!(
            mux.take_opened_links(),
            vec![Link { id: link, trust }]
        );
        block(mux.send(link, hello())).unwrap();
        client.pump(port.slot(index));
        assert!(client.next_on(CH_PROTO).is_some(), "the hello");
        (link, client)
    }

    /// A host's secure end on a network link, at `ws()`.
    #[cfg(feature = "wifi")]
    struct SecureClient {
        link: lp_link::Link<SelectiveRepeat>,
        events: VecDeque<LinkEvent>,
        now: Micros,
    }

    #[cfg(feature = "wifi")]
    impl SecureClient {
        fn pump(&mut self, slot: &RadioLinkSlot) {
            let mut quiet = 0;
            for _ in 0..4_000 {
                self.now += 5_000;
                let t = self.now;
                let mut moved = false;
                while let Some(frame) = slot.poll_frame(t, <[u8]>::to_vec) {
                    self.link.on_datagram(t, &frame);
                    moved = true;
                }
                while let Some(frame) = self.link.poll_transmit(t).map(<[u8]>::to_vec) {
                    slot.on_datagram(t, &frame);
                    moved = true;
                }
                while let Some(event) = self.link.recv() {
                    self.events.push_back(event);
                }
                quiet = if moved { 0 } else { quiet + 1 };
                if quiet >= 10 {
                    break;
                }
            }
        }

        fn next_update(&mut self) -> Option<Vec<u8>> {
            self.next_on(CH_UPDATE)
        }

        fn next_on(&mut self, wanted: u8) -> Option<Vec<u8>> {
            while let Some(event) = self.events.pop_front() {
                if let LinkEvent::Message { channel, data } = event
                    && channel == wanted
                {
                    return Some(data);
                }
            }
            None
        }
    }

    #[cfg(feature = "wifi")]
    fn fill(buf: &mut [u8]) {
        for (i, b) in buf.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(13).wrapping_add(5);
        }
    }

    /// [`open_link`], then bring the session up and deliver its hello.
    fn open_session<D: DelayNs>(
        port: &'static RadioLinkPort,
        mux: &mut LinkMuxTransport<Usb, D>,
        central: &mut Central,
        slot: usize,
        att_mtu: u16,
    ) -> LinkId {
        let link = open_link(port, mux, slot, att_mtu);
        central.pump(port.slot(slot));
        assert!(central.saw_up());
        assert_eq!(mux.take_opened_links(), vec![RadioLinkPort::link(link)]);
        block(mux.send(link, hello())).unwrap();
        central.pump(port.slot(slot));
        assert!(central.next_proto().is_some(), "the hello");
        link
    }

    /// The central's end: Studio's lp-link session over one GATT connection,
    /// at the unmodified `ble()` preset, on a clock of its own (far ahead of
    /// the embassy clock the mux reads, so the board never looks stalled).
    /// Every frame either way must fit one ATT value at `att_mtu`.
    struct Central {
        link: lp_link::Link<SelectiveRepeat>,
        events: VecDeque<LinkEvent>,
        now: Micros,
        value_max: usize,
        /// The longest frame seen either way.
        max_frame: usize,
        /// The most the board's link held after any round of a pump.
        peak_ram: usize,
    }

    impl Central {
        fn new(nonce: u32, att_mtu: u16) -> Self {
            Self {
                link: lp_link::Link::new(LinkConfig::ble(), nonce),
                events: VecDeque::new(),
                now: 1_000_000_000_000,
                value_max: usize::from(att_mtu) - 3,
                max_frame: 0,
                peak_ram: 0,
            }
        }

        /// Move frames both ways, 5 ms of link time a round, until the air
        /// has been quiet for 50 ms (the delayed ACK included).
        fn pump(&mut self, slot: &RadioLinkSlot) {
            let mut quiet = 0;
            for _ in 0..4_000 {
                self.now += 5_000;
                let t = self.now;
                let mut moved = false;
                while let Some(frame) = slot.poll_frame(t, <[u8]>::to_vec) {
                    self.note_frame(frame.len());
                    self.link.on_datagram(t, &frame);
                    moved = true;
                }
                while let Some(frame) = self.link.poll_transmit(t).map(<[u8]>::to_vec) {
                    self.note_frame(frame.len());
                    slot.on_datagram(t, &frame);
                    moved = true;
                }
                while let Some(event) = self.link.recv() {
                    self.events.push_back(event);
                }
                self.peak_ram = self.peak_ram.max(slot.ram_bytes().unwrap_or(0));
                quiet = if moved { 0 } else { quiet + 1 };
                if quiet >= 10 {
                    break;
                }
            }
        }

        fn note_frame(&mut self, len: usize) {
            assert!(
                len <= self.value_max,
                "a {len} B frame does not fit one {} B ATT value",
                self.value_max
            );
            self.max_frame = self.max_frame.max(len);
        }

        fn saw_up(&mut self) -> bool {
            let at = self
                .events
                .iter()
                .position(|e| matches!(e, LinkEvent::Up { .. }));
            match at {
                Some(at) => {
                    self.events.drain(..=at);
                    true
                }
                None => false,
            }
        }

        fn next_proto(&mut self) -> Option<Vec<u8>> {
            self.next_on(CH_PROTO)
        }

        fn next_update(&mut self) -> Option<Vec<u8>> {
            self.next_on(CH_UPDATE)
        }

        /// The next message on `wanted`, dropping everything before it.
        fn next_on(&mut self, wanted: u8) -> Option<Vec<u8>> {
            while let Some(event) = self.events.pop_front() {
                if let LinkEvent::Message { channel, data } = event
                    && channel == wanted
                {
                    return Some(data);
                }
            }
            None
        }

        fn send_request(&mut self, msg: &lpc_wire::ClientMessage) {
            let json = lpc_wire::json::to_string(msg).unwrap();
            self.link.send(CH_PROTO, json.as_bytes()).unwrap();
        }
    }

    fn hello_request(id: u64) -> lpc_wire::ClientMessage {
        lpc_wire::ClientMessage {
            id,
            msg: lpc_wire::ClientRequest::Hello,
        }
    }

    fn hello() -> WireServerMessage {
        WireServerMessage::new(
            0,
            ServerMsgBody::Hello(lpc_wire::ServerHello {
                proto: lpc_wire::WIRE_PROTO_VERSION,
                build: lpc_wire::BuildFacts {
                    features: vec![],
                    package: "fw-esp32c6".to_string(),
                    version: "unknown".into(),
                    commit: "unknown".to_string(),
                    dirty: false,
                    profile: "release-esp32".to_string(),
                },
                hardware: lpc_wire::HardwareFacts::default(),
                device_uid: None,
                pack_format: lpc_wire::PACK_FORMAT_VERSION,
                auth: lpc_wire::HelloAuth::TRUSTED,
                firmware: None,
            }),
        )
    }

    fn error_reply(id: u64, len: usize) -> WireServerMessage {
        WireServerMessage::new(
            id,
            ServerMsgBody::Error {
                error: "x".repeat(len),
            },
        )
    }

    #[cfg(feature = "json-pack")]
    fn log_reply(n: u64) -> WireServerMessage {
        WireServerMessage::new(
            n,
            ServerMsgBody::Log {
                level: lpc_wire::server::api::LogLevel::Info,
                message: alloc::format!("line {n}"),
            },
        )
    }

    #[cfg(feature = "json-pack")]
    fn set_encoding(encoding: WireEncoding) -> WireServerMessage {
        WireServerMessage::new(
            lpc_wire::PACK_OPT_IN_REQUEST_ID,
            ServerMsgBody::SetEncoding { encoding },
        )
    }

    fn heartbeat(link: Option<lpc_wire::server::LinkCounters>) -> WireServerMessage {
        WireServerMessage::new(
            0,
            ServerMsgBody::Heartbeat {
                fps: lpc_wire::server::SampleStats {
                    avg: 0.0,
                    sdev: 0.0,
                    min: 0.0,
                    max: 0.0,
                },
                frame_count: 0,
                loaded_projects: Vec::new(),
                uptime_ms: 0,
                memory: None,
                recovery: None,
                outputs: None,
                link,
                identity: None,
            },
        )
    }

    fn block<F: core::future::Future>(future: F) -> F::Output {
        embassy_futures::block_on(future)
    }

    /// A deadline that never elapses.
    struct NeverDelay;
    impl DelayNs for NeverDelay {
        async fn delay_ns(&mut self, _ns: u32) {
            core::future::pending::<()>().await;
        }
    }

    /// A deadline that elapses at once, counting the nanoseconds asked for.
    struct CountingDelay(Rc<core::cell::Cell<u64>>);
    impl DelayNs for CountingDelay {
        async fn delay_ns(&mut self, ns: u32) {
            self.0.set(self.0.get() + u64::from(ns));
        }
    }

    /// A deadline that has always already elapsed.
    struct ReadyDelay;
    impl DelayNs for ReadyDelay {
        async fn delay_ns(&mut self, _ns: u32) {}
    }

    /// The USB transport's stand-in. With `port`, it notes every send made
    /// while a radio link still read the frame buffer (there must be none).
    #[derive(Default)]
    struct Usb {
        inbox: Vec<Incoming>,
        sent: Vec<u64>,
        opened: Vec<Link>,
        port: Option<&'static RadioLinkPort>,
        sent_while_held: usize,
    }

    impl FrameBufHolder for Usb {}

    impl LinkUpkeep for Usb {
        fn take_opened_links(&mut self) -> Vec<Link> {
            core::mem::take(&mut self.opened)
        }
    }

    impl ServerTransport for Usb {
        async fn send(
            &mut self,
            _link: LinkId,
            msg: WireServerMessage,
        ) -> Result<(), TransportError> {
            if self
                .port
                .is_some_and(|port| port.slots().any(RadioLinkSlot::external_in_flight))
            {
                self.sent_while_held += 1;
            }
            self.sent.push(msg.id);
            Ok(())
        }
        async fn receive(&mut self) -> Result<Option<Incoming>, TransportError> {
            Ok(self.inbox.pop())
        }
        async fn receive_all(&mut self) -> Result<Vec<Incoming>, TransportError> {
            Ok(core::mem::take(&mut self.inbox))
        }
        fn links(&self) -> Vec<Link> {
            vec![Link::PRIMARY]
        }
        async fn close(&mut self) -> Result<(), TransportError> {
            Ok(())
        }
    }
}
