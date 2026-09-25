//! The link mux: one [`ServerTransport`] over the USB cable and up to
//! [`RADIO_LINK_SLOTS`] radio links.
//!
//! - **USB** is the primary transport it wraps ([`LinkId::PRIMARY`],
//!   trusted), unchanged: everything addressed to the primary link goes
//!   straight to it.
//! - **Radio links** arrive and leave through the [`RadioLinkPort`], one
//!   [`LinkId`] each, all untrusted. The server's gate (M3) decides what an
//!   untrusted link may do; this file only decides which bytes go where.
//!
//! Three rules live here because this is the edge that owns them:
//!
//! 1. **One frame in flight, overall.** A radio frame is serialized into the
//!    same static frame buffer USB uses and the send waits for the radio side
//!    to finish with it, exactly as the USB write does. No per-link 16 KiB
//!    buffer exists.
//! 2. **A slow radio link cannot stall the device for long.** The wait is
//!    bounded ([`RADIO_WRITE_DEADLINE_MS`]); past it the frame's lease is
//!    revoked, the link is closed with a logged reason, and the server loop
//!    moves on.
//! 3. **A radio link must log in within [`LOGIN_DEADLINE_MS`]** of opening,
//!    unless the device is `open` — [`LinkMuxTransport::expire_unauthenticated`],
//!    driven from the server loop, which holds the clock and the server.
//!
//! A radio send that fails does **not** return an error to the server. The
//! server's `tick_and_send` stops answering the whole batch on the first
//! transport error, and one dying radio link must not cost the USB cable its
//! replies. The failure is logged once, at error level, with the link, the
//! frame id and the reason, and the link is closed — its session is dropped
//! on the next tick ([`ServerTransport::take_closed_links`]); frames still
//! addressed to it are skipped at debug level.

use alloc::vec::Vec;

use embassy_futures::select::{Either, select};
use embedded_hal_async::delay::DelayNs;
use lpa_server::LpServer;
use lpc_shared::transport::{Incoming, Link, LinkId, ServerTransport};
use lpc_wire::{TransportError, WireServerMessage};

use super::radio_link_port::{RADIO_LINK_SLOTS, RadioLinkEvent, RadioLinkPort, RadioWriteRequest};
use crate::link_upkeep::LinkUpkeep;
use crate::serial::server_msg::serialize_server_msg;
use crate::transport::parse_wire_line;

/// How long a radio link has to finish one frame before it is closed.
///
/// The largest frame is the 16 KiB project-read budget. The slowest central
/// measured (a MacBook, spike Run C) took notifications at 5–12 KB/s, so the
/// worst honest frame needs ~3.3 s; the bound is set above that, not at it.
pub const RADIO_WRITE_DEADLINE_MS: u32 = 5_000;

/// How long an untrusted radio link may stay open without logging in (PQ6).
pub const LOGIN_DEADLINE_MS: u64 = 10_000;

/// One open radio link, as the mux tracks it.
#[derive(Debug, Clone, Copy)]
struct RadioLink {
    id: LinkId,
    slot: usize,
    /// Server-loop time the link was first seen by the upkeep, which starts
    /// its login deadline.
    opened_at_ms: Option<u64>,
    /// It held a tier once; the deadline no longer applies.
    cleared: bool,
    /// What this link's replies are written in: JSON until the server
    /// answers its `SetEncoding` opt-in, then what that answer names — the
    /// same rule the USB transport keeps (plan `lp-json-pack`). A radio link
    /// that closes takes its encoding with it; the next one starts at JSON.
    encoding: lpc_wire::WireEncoding,
}

/// The USB transport plus the radio links, as one [`ServerTransport`].
pub struct LinkMuxTransport<U, D> {
    primary: U,
    port: &'static RadioLinkPort,
    delay: D,
    radio: Vec<RadioLink>,
    /// Opened links whose hello is still owed.
    opened: Vec<Link>,
    /// Closed links the server has not been told about yet.
    closed: Vec<LinkId>,
    generation: u32,
    upkeep_hook: Option<fn(&LpServer, u64)>,
}

impl<U: ServerTransport, D: DelayNs> LinkMuxTransport<U, D> {
    /// Wrap `primary` (the USB transport) and serve the radio links that
    /// `port` announces. `delay` bounds a radio write.
    pub fn new(primary: U, port: &'static RadioLinkPort, delay: D) -> Self {
        Self {
            primary,
            port,
            delay,
            radio: Vec::new(),
            opened: Vec::new(),
            closed: Vec::new(),
            generation: 0,
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
    /// without holding a tier. `now_ms` is the server loop's clock;
    /// `has_tier` asks the server (`LpServer::link_tier(..).is_some()`).
    pub fn expire_unauthenticated(&mut self, now_ms: u64, has_tier: impl Fn(Link) -> bool) {
        let mut expired = Vec::new();
        for link in &mut self.radio {
            if link.cleared {
                continue;
            }
            let opened_at = *link.opened_at_ms.get_or_insert(now_ms);
            if has_tier(RadioLinkPort::link(link.id)) {
                link.cleared = true;
            } else if now_ms.saturating_sub(opened_at) >= LOGIN_DEADLINE_MS {
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
                        encoding: lpc_wire::WireEncoding::Json,
                    });
                    self.opened.push(RadioLinkPort::link(link));
                }
                RadioLinkEvent::Closed { link } => {
                    if let Some(at) = self.radio.iter().position(|l| l.id == link) {
                        self.radio.remove(at);
                        self.opened.retain(|l| l.id != link);
                        self.closed.push(link);
                        log::info!("radio link {link}: closed");
                    }
                }
            }
        }
    }

    /// Drop `id` from the mux, ask the radio side to disconnect it, and owe
    /// the server a closed notice.
    fn close_radio(&mut self, id: LinkId, reason: &'static str) {
        if let Some(at) = self.radio.iter().position(|l| l.id == id) {
            let link = self.radio.remove(at);
            self.opened.retain(|l| l.id != id);
            self.port.slot(link.slot).request_close(reason);
            self.closed.push(id);
        }
    }

    async fn send_radio(
        &mut self,
        link: LinkId,
        msg: WireServerMessage,
    ) -> Result<(), TransportError> {
        let Some((slot, link_encoding)) = self
            .radio
            .iter()
            .find(|l| l.id == link)
            .map(|l| (l.slot, l.encoding))
        else {
            log::debug!("radio link {link}: gone, skipping frame id={}", msg.id);
            return Ok(());
        };
        let id = msg.id;
        // The answer to an opt-in is always JSON (the host reads it before it
        // knows the outcome); the switch it announces applies to every frame
        // after it, as on the USB transport.
        let (encoding, switch_to) = match msg.msg {
            lpc_wire::server::ServerMsgBody::SetEncoding { encoding } => {
                (lpc_wire::WireEncoding::Json, Some(encoding))
            }
            _ => (link_encoding, None),
        };
        // Same buffer, same exclusivity argument as the USB write: the send
        // below does not return until the radio side is done with it or the
        // lease is revoked.
        let len = serialize_server_msg(&msg, encoding)?;
        drop(msg);
        let generation = self.generation;
        self.generation = self.generation.wrapping_add(1);
        self.port.lease_frame(generation);
        self.port.submit_write(
            slot,
            RadioWriteRequest {
                link,
                generation,
                len,
            },
        );
        let outcome = select(
            self.port.write_result(generation),
            self.delay.delay_ms(RADIO_WRITE_DEADLINE_MS),
        )
        .await;
        // Whatever happened, the buffer is the server's again from here.
        self.port.revoke_frame();
        match outcome {
            Either::First(Ok(())) => {
                if let Some(encoding) = switch_to
                    && let Some(l) = self.radio.iter_mut().find(|l| l.id == link)
                {
                    l.encoding = encoding;
                    log::info!("radio link {link}: replies are now {}", encoding.as_str());
                }
                Ok(())
            }
            Either::First(Err(error)) => {
                log::error!(
                    "radio link {link}: dropping frame id={id} ({len} B): {error} — closing"
                );
                self.close_radio(link, "write failed");
                Ok(())
            }
            Either::Second(()) => {
                self.port.withdraw_write(slot);
                log::error!(
                    "radio link {link}: frame id={id} ({len} B) not sent within \
                     {RADIO_WRITE_DEADLINE_MS} ms — closing"
                );
                self.close_radio(link, "write deadline");
                Ok(())
            }
        }
    }
}

impl<U: ServerTransport, D: DelayNs> ServerTransport for LinkMuxTransport<U, D> {
    async fn send(&mut self, link: LinkId, msg: WireServerMessage) -> Result<(), TransportError> {
        if link == LinkId::PRIMARY {
            self.primary.send(link, msg).await
        } else {
            self.send_radio(link, msg).await
        }
    }

    async fn receive(&mut self) -> Result<Option<Incoming>, TransportError> {
        if let Some(incoming) = self.primary.receive().await? {
            return Ok(Some(incoming));
        }
        // Events before lines: a link's Opened always precedes its lines,
        // and its Closed always follows them.
        self.drain_events();
        while let Some((link, line)) = self.port.try_line() {
            if !self.radio.iter().any(|l| l.id == link) {
                log::debug!("radio link {link}: gone, dropping a {} B line", line.len());
                continue;
            }
            if let Some(msg) = parse_wire_line(&line) {
                return Ok(Some(Incoming::on(RadioLinkPort::link(link), msg)));
            }
        }
        Ok(None)
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

impl<U: ServerTransport, D: DelayNs> LinkUpkeep for LinkMuxTransport<U, D> {
    fn take_opened_links(&mut self) -> Vec<Link> {
        core::mem::take(&mut self.opened)
    }

    fn upkeep(&mut self, server: &LpServer, now_ms: u64) {
        self.expire_unauthenticated(now_ms, |link| server.link_tier(link).is_some());
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
    use lpc_shared::transport::LinkTrust;

    extern crate std;

    /// The frame buffer is one static; tests that serialize into it take
    /// turns.
    static FRAME_BUF_TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn frame_buf_turn() -> std::sync::MutexGuard<'static, ()> {
        FRAME_BUF_TURN.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn a_radio_link_opens_carries_a_request_and_closes() {
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        let link = port.mint_link();
        assert_ne!(link, LinkId::PRIMARY);
        block(port.announce(RadioLinkEvent::Opened { link, slot: 0 }));
        assert!(port.deliver_line(link, hello_line(7)));

        let incoming = block(mux.receive()).unwrap().expect("the radio line");
        assert_eq!(incoming.link, link);
        assert_eq!(incoming.trust, LinkTrust::Untrusted);
        assert_eq!(incoming.msg.id, 7);
        assert_eq!(
            mux.take_opened_links(),
            vec![RadioLinkPort::link(link)],
            "a joining link is owed its own hello"
        );
        assert_eq!(mux.links().len(), 2);

        block(port.announce(RadioLinkEvent::Closed { link }));
        assert!(block(mux.receive()).unwrap().is_none());
        assert_eq!(mux.take_closed_links(), vec![link]);
        assert_eq!(mux.links(), vec![Link::PRIMARY]);
    }

    #[test]
    fn a_radio_frame_is_handed_over_under_a_lease_and_the_lease_ends() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        let link = open_link(port, &mut mux, 1);

        // The radio side: take the request, copy the frame out, finish.
        let radio = async {
            let request = port.slot(1).next_write().await;
            let mut frame = vec![0u8; request.len];
            assert!(port.copy_frame(&request, 0, &mut frame));
            port.finish_write(&request, Ok(()));
            (request, frame)
        };
        let (sent, (request, frame)) = block(embassy_futures::join::join(
            mux.send(link, WireServerMessage::new(3, heartbeat_body())),
            radio,
        ));
        sent.unwrap();
        assert_eq!(request.link, link);
        let text = core::str::from_utf8(&frame).unwrap();
        assert!(text.starts_with("\nM!{"), "{text}");
        assert!(text.ends_with("}\n"), "{text}");
        // After the send the lease is gone: a late copy reads nothing.
        let mut late = vec![0u8; 4];
        assert!(!port.copy_frame(&request, 0, &mut late));
    }

    #[test]
    fn a_failed_radio_write_closes_the_link_without_failing_the_send() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        let link = open_link(port, &mut mux, 0);
        let radio = async {
            let request = port.slot(0).next_write().await;
            port.finish_write(&request, Err(TransportError::ConnectionLost));
        };
        let (sent, ()) = block(embassy_futures::join::join(
            mux.send(link, WireServerMessage::new(4, heartbeat_body())),
            radio,
        ));
        assert!(sent.is_ok(), "one radio link's failure is not the server's");
        assert_eq!(mux.take_closed_links(), vec![link]);
        assert_eq!(port.slot(0).take_close_request(), Some("write failed"));
        // Frames still addressed to it are skipped, not errors.
        assert!(block(mux.send(link, WireServerMessage::new(5, heartbeat_body()))).is_ok());
    }

    #[test]
    fn a_radio_write_past_its_deadline_closes_the_link() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, ReadyDelay);
        let link = open_link(port, &mut mux, 0);
        // Nobody services the slot; the (immediately elapsed) deadline wins.
        assert!(block(mux.send(link, WireServerMessage::new(6, heartbeat_body()))).is_ok());
        assert_eq!(mux.take_closed_links(), vec![link]);
        assert_eq!(port.slot(0).take_close_request(), Some("write deadline"));
        assert!(
            !port.slot(0).has_pending_write(),
            "the abandoned request is withdrawn"
        );
    }

    #[test]
    fn an_unauthenticated_link_is_closed_after_the_deadline() {
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        let link = open_link(port, &mut mux, 0);
        mux.expire_unauthenticated(1_000, |_| false);
        mux.expire_unauthenticated(1_000 + LOGIN_DEADLINE_MS - 1, |_| false);
        assert!(mux.take_closed_links().is_empty());
        mux.expire_unauthenticated(1_000 + LOGIN_DEADLINE_MS, |_| false);
        assert_eq!(mux.take_closed_links(), vec![link]);
        assert_eq!(
            port.slot(0).take_close_request(),
            Some("no login within the deadline")
        );
    }

    #[test]
    fn a_link_that_logs_in_is_never_expired() {
        let port = leak_port();
        let mut mux = LinkMuxTransport::new(Usb::default(), port, NeverDelay);
        let _link = open_link(port, &mut mux, 0);
        mux.expire_unauthenticated(0, |_| false);
        mux.expire_unauthenticated(5_000, |_| true);
        mux.expire_unauthenticated(60_000, |_| false);
        assert!(mux.take_closed_links().is_empty());
    }

    #[test]
    fn usb_traffic_passes_through_untouched() {
        let _turn = frame_buf_turn();
        let port = leak_port();
        let mut usb = Usb::default();
        usb.inbox.push(Incoming::primary(hello_message(9)));
        let mut mux = LinkMuxTransport::new(usb, port, NeverDelay);
        let incoming = block(mux.receive()).unwrap().unwrap();
        assert_eq!(incoming.link, LinkId::PRIMARY);
        block(mux.send(LinkId::PRIMARY, WireServerMessage::new(9, heartbeat_body()))).unwrap();
        assert_eq!(mux.primary.sent, vec![9]);
    }

    // ---- helpers ----

    fn leak_port() -> &'static RadioLinkPort {
        Box::leak(Box::new(RadioLinkPort::new()))
    }

    fn open_link<D: DelayNs>(
        port: &'static RadioLinkPort,
        mux: &mut LinkMuxTransport<Usb, D>,
        slot: usize,
    ) -> LinkId {
        let link = port.mint_link();
        block(port.announce(RadioLinkEvent::Opened { link, slot }));
        assert!(block(mux.receive()).unwrap().is_none());
        assert_eq!(mux.take_opened_links().len(), 1);
        link
    }

    fn hello_message(id: u64) -> lpc_wire::ClientMessage {
        lpc_wire::ClientMessage {
            id,
            msg: lpc_wire::ClientRequest::Hello,
        }
    }

    fn hello_line(id: u64) -> alloc::string::String {
        let json = lpc_wire::json::to_string(&hello_message(id)).unwrap();
        let mut line = "M!".to_string();
        line.push_str(&json);
        line
    }

    fn heartbeat_body() -> lpc_wire::server::ServerMsgBody {
        lpc_wire::server::ServerMsgBody::Error {
            error: "x".to_string(),
        }
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

    #[derive(Default)]
    struct Usb {
        inbox: Vec<Incoming>,
        sent: Vec<u64>,
    }

    impl ServerTransport for Usb {
        async fn send(
            &mut self,
            _link: LinkId,
            msg: WireServerMessage,
        ) -> Result<(), TransportError> {
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
