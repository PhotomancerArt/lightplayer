//! The link mux: one [`ServerTransport`] over the USB cable and up to
//! [`RADIO_LINK_SLOTS`] radio links.
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
//! Four rules live here because this is the edge that owns them:
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
//!    instead, and holds nothing.
//! 2. **A slow radio link cannot stall the device for long.** The wait for a
//!    radio link to let go of the frame buffer is bounded
//!    ([`RADIO_WRITE_DEADLINE_MS`]); past it the link is closed with a
//!    logged reason (its link dropped at once, so it reads the buffer no
//!    more) and the server loop moves on.
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
use lp_link::{CH_PROTO, LinkEvent, LinkState, Micros, ResetReason, SendError};
use lpa_server::LpServer;
use lpc_shared::transport::{Incoming, Link, LinkId, ServerTransport};
use lpc_wire::server::ServerMsgBody;
use lpc_wire::{LinkCounterTally, TransportError, WireServerMessage};

use super::frame_buf_holder::FrameBufHolder;
use super::radio_link_config::SMALL_REPLY_BYTES;
use super::radio_link_port::{RADIO_LINK_SLOTS, RadioLinkEvent, RadioLinkPort, RadioLinkSlot};
use crate::link_upkeep::LinkUpkeep;
use crate::serial::packed_link::PackedLink;
use crate::serial::server_msg::frame_bytes;
use crate::serial::server_payload::{decode_client_payload, serialize_server_payload};

/// How long a radio link has to finish taking a long reply out of the frame
/// buffer once someone else needs the buffer, before it is closed.
///
/// The largest reply is the 16 KiB project-read budget. The slowest central
/// measured (a MacBook, spike Run C) took notifications at 5–12 KB/s, so the
/// worst honest reply needs ~3.3 s; the bound is set above that, not at it.
pub const RADIO_WRITE_DEADLINE_MS: u32 = 5_000;

/// How long an untrusted radio link may stay open without logging in (PQ6).
pub const LOGIN_DEADLINE_MS: u64 = 10_000;

/// One open radio link, as the mux tracks it.
struct RadioLink {
    id: LinkId,
    slot: usize,
    /// Server-loop time the link was first seen by the upkeep, which starts
    /// its login deadline.
    opened_at_ms: Option<u64>,
    /// It held a tier once; the deadline no longer applies.
    cleared: bool,
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
    upkeep_hook: Option<fn(&LpServer, u64)>,
}

impl<U: ServerTransport + FrameBufHolder, D: DelayNs> LinkMuxTransport<U, D> {
    /// Wrap `primary` (the USB transport) and serve the radio links that
    /// `port` announces. `delay` bounds a radio link's hold on the frame
    /// buffer.
    pub fn new(primary: U, port: &'static RadioLinkPort, delay: D) -> Self {
        Self {
            primary,
            port,
            delay,
            radio: Vec::new(),
            inbox: VecDeque::new(),
            closed: Vec::new(),
            upkeep_hook: None,
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
        for link in &mut self.radio {
            if link.cleared {
                continue;
            }
            let opened_at = *link.opened_at_ms.get_or_insert(now_ms);
            let wire_link = RadioLinkPort::link(link.id);
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
                    if slot >= RADIO_LINK_SLOTS || self.radio.iter().any(|l| l.slot == slot) {
                        // Cannot happen with a well-behaved radio side; if it
                        // does, refuse the newcomer rather than cross wires.
                        log::error!("radio link {link}: slot {slot} unusable — closing");
                        if slot < RADIO_LINK_SLOTS {
                            self.port.slot(slot).request_close("slot already in use");
                        }
                        continue;
                    }
                    log::info!("radio link {link}: opened (slot {slot})");
                    self.radio.push(RadioLink {
                        id: link,
                        slot,
                        opened_at_ms: None,
                        cleared: false,
                        session: None,
                        hello_owed: false,
                        packed: PackedLink::new(),
                        tally: LinkCounterTally::new(),
                    });
                }
                RadioLinkEvent::Closed { link } => {
                    if let Some(at) = self.radio.iter().position(|l| l.id == link) {
                        self.radio.remove(at);
                        self.inbox.retain(|i| i.link != link);
                        self.closed.push(link);
                        log::info!("radio link {link}: closed");
                    }
                }
            }
        }
    }

    /// Take each radio link's events: client messages into the inbox,
    /// lifecycle into the link's state. A link whose session just came up
    /// is taken no further until its hello is handed out (rule 3).
    #[inline(never)]
    fn pump_radio(&mut self) {
        let port = self.port;
        let now = now_us();
        for radio in &mut self.radio {
            let slot = port.slot(radio.slot);
            while !radio.hello_owed {
                let Some(Some(event)) = slot.with_link(radio.id, |link| link.recv()) else {
                    break;
                };
                match event {
                    LinkEvent::Message { channel, data } if channel == CH_PROTO => {
                        let decoded = decode_client_payload(&data);
                        match decoded {
                            Ok(msg) => {
                                // The request's bytes go before the inbox can
                                // grow (the USB link's reason: a growth above
                                // them would pin their hole).
                                drop(data);
                                log::debug!("radio link {}: received id={}", radio.id, msg.id);
                                self.inbox
                                    .push_back(Incoming::on(RadioLinkPort::link(radio.id), msg));
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
                        radio.packed.back_to_json();
                        radio.session = Some(generation);
                        radio.hello_owed = true;
                        log::info!("radio link {}: session {generation} up", radio.id);
                    }
                    LinkEvent::Reset { reason, generation } => {
                        radio.packed.back_to_json();
                        radio.session = None;
                        radio.tally.note_reset(reason);
                        // Requests from the ended session: its host fails
                        // them itself, and a reply would reach the next one.
                        let id = radio.id;
                        self.inbox.retain(|i| i.link != id);
                        match reason {
                            ResetReason::PeerRestarted | ResetReason::Requested => log::info!(
                                "radio link {id}: session reset ({reason:?}); now {generation}"
                            ),
                            ResetReason::RetryLimit | ResetReason::ProtocolError => log::warn!(
                                "radio link {id}: session reset ({reason:?}); now {generation}"
                            ),
                        }
                    }
                }
            }
            if let Some(stalled) = slot.with_link(radio.id, |link| link.is_stalled(now)) {
                radio.tally.note_stalled(stalled);
            }
        }
    }

    /// Drop `id` from the mux, drop its link (it stops reading the frame
    /// buffer now), ask the radio side to disconnect it, and owe the server a
    /// closed notice.
    fn close_radio(&mut self, id: LinkId, reason: &'static str) {
        if let Some(at) = self.radio.iter().position(|l| l.id == id) {
            let link = self.radio.remove(at);
            let slot = self.port.slot(link.slot);
            slot.drop_link(id);
            slot.request_close(reason);
            self.inbox.retain(|i| i.link != id);
            self.closed.push(id);
        }
    }

    /// Wait until no radio link reads the frame buffer (rule 1): each one
    /// with a long reply in flight gets [`RADIO_WRITE_DEADLINE_MS`] to finish
    /// cutting it into frames, and is closed past that.
    async fn release_radio_holders(&mut self) {
        let port = self.port;
        for index in 0..RADIO_LINK_SLOTS {
            let slot = port.slot(index);
            if !slot.external_in_flight() {
                continue;
            }
            let outcome = select(
                wait_for_release(slot),
                self.delay.delay_ms(RADIO_WRITE_DEADLINE_MS),
            )
            .await;
            if let Either::Second(()) = outcome {
                let Some((id, held)) = slot.with_any_link(|id, link| (id, link.buffered_bytes()))
                else {
                    continue;
                };
                log::error!(
                    "radio link {id}: a reply still not out of the frame buffer after \
                     {RADIO_WRITE_DEADLINE_MS} ms ({held} B held) — closing"
                );
                slot.drop_link(id);
                self.close_radio(id, "reply deadline");
            }
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
        links.extend(self.radio.iter().map(|l| RadioLinkPort::link(l.id)));
        links
    }

    fn take_closed_links(&mut self) -> Vec<LinkId> {
        let mut closed = self.primary.take_closed_links();
        closed.append(&mut self.closed);
        closed
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
            if core::mem::take(&mut radio.hello_owed) {
                opened.push(RadioLinkPort::link(radio.id));
            }
        }
        opened
    }

    fn upkeep(&mut self, server: &LpServer, now_ms: u64) {
        self.primary.upkeep(server, now_ms);
        self.expire_unauthenticated(
            now_ms,
            |link| server.link_tier(link).is_some(),
            |link| server.login_pending(link),
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
    use alloc::string::ToString;
    use alloc::vec;
    use lp_link::{LinkConfig, SelectiveRepeat};
    use lpc_shared::transport::LinkTrust;
    #[cfg(feature = "json-pack")]
    use lpc_wire::WireEncoding;

    extern crate std;

    use crate::serial::server_msg::frame_buf_turn;

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

        let cfg = super::super::radio_link_config(247).unwrap();
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

    #[test]
    fn an_unauthenticated_link_is_closed_after_the_deadline() {
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        let link = open_link(port, &mut mux, 0, 247);
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
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        let _link = open_link(port, &mut mux, 0, 247);
        mux.expire_unauthenticated(0, |_| false, |_| false);
        mux.expire_unauthenticated(5_000, |_| true, |_| false);
        mux.expire_unauthenticated(60_000, |_| false, |_| false);
        assert!(mux.take_closed_links().is_empty());
    }

    #[test]
    fn an_outstanding_login_holds_the_deadline_until_it_ends() {
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        let link = open_link(port, &mut mux, 0, 247);
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

    // ---- helpers ----

    fn leak_port() -> &'static RadioLinkPort {
        Box::leak(Box::new(RadioLinkPort::new()))
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
        port.slot(slot)
            .open(link, att_mtu, 0x5eed_0000 + slot as u32)
            .unwrap();
        block(port.announce(RadioLinkEvent::Opened { link, slot }));
        assert!(block(mux.receive()).unwrap().is_none());
        link
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
            while let Some(event) = self.events.pop_front() {
                if let LinkEvent::Message { channel, data } = event
                    && channel == CH_PROTO
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
