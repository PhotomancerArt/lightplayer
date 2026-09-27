//! The server's transport over the USB host link: whole wire messages on
//! lp-link's proto channel.
//!
//! - **Receive**: each proto-channel message is one client message as JSON
//!   bytes — no `M!`, no line splitting; the link delivered it whole and
//!   checked.
//! - **Send**: each reply is serialized in thread context into the static
//!   frame buffer as one proto payload ([`crate::serial::server_payload`]:
//!   JSON, or `L` + a learned packed frame on a link whose host opted in)
//!   and queued with `Link::send`, which copies it. The link then delivers it
//!   or resets the session; either way the host learns which.
//!
//! Three rules live here because this is the edge that sees the link's
//! lifecycle (plan `lp-link-usb-cutover`):
//!
//! 1. **Per-session wire state resets with the session (D4).** On every `Up`
//!    and `Reset` the link's replies go back to JSON and the learned table is
//!    freed; a host re-opts-in after `Up`. There is no link epoch, no desync
//!    re-ask and no resync marker: over a link that resends every lost byte a
//!    table can only lose step with its host's if the session ends, and then
//!    both ends reset together.
//! 2. **The hello is the first message of every session (D5).** On `Up` the
//!    transport stops taking the host's messages and refuses every other
//!    send until the owed hello has gone out (the server loop asks for it
//!    through [`LinkUpkeep::take_opened_links`] at the top of its next pass).
//! 3. **A full send budget is waited out, then dropped (D8).** While the host
//!    is draining, a reply the budget cannot take yet waits for room
//!    ([`SEND_ROOM_WAIT`] at most); a stalled host's reply is dropped at once,
//!    counted, and the peer told. The old not-draining latch, which dropped
//!    replies on a write timeout, is gone.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use embassy_time::{Duration, Instant, Timer};
use lp_link::{LinkEvent, LinkState, ResetReason, SendError};
use lpc_shared::transport::{Incoming, Link, LinkId, ServerTransport};
use lpc_wire::server::ServerMsgBody;
use lpc_wire::{ClientMessage, TransportError, WireServerMessage};

use super::usb_link_counters;
use super::usb_link_shared::UsbLinkShared;
use super::usb_link_task::now_us;
use crate::link_upkeep::LinkUpkeep;
use crate::serial::packed_link::PackedLink;
use crate::serial::server_msg::frame_bytes;
use crate::serial::server_payload::serialize_server_payload;

/// Longest a reply waits for room in the link's send budget while the host
/// is draining. Past it the reply is dropped as if the host had stalled.
pub const SEND_ROOM_WAIT: Duration = Duration::from_secs(3);

/// How often a waiting reply looks for room again.
const SEND_ROOM_POLL: Duration = Duration::from_millis(2);

/// The USB host link's server transport. See the module docs.
pub struct UsbLinkTransport {
    shared: &'static UsbLinkShared,
    /// What this session's replies are written in, and its learned table
    /// while packed.
    packed: PackedLink,
    /// Client messages taken off the link, waiting for `receive`.
    inbox: VecDeque<ClientMessage>,
    /// The link came up and its hello has not been handed out yet.
    hello_owed: bool,
}

/// What queueing one payload came to.
enum Queued {
    Yes,
    /// The session the payload was coded for is gone (a reset, or no host).
    NoSession,
    /// The send budget is full and waiting is not an option.
    Full,
    /// Larger than the link carries in one message.
    TooBig,
}

impl UsbLinkTransport {
    /// The transport over `shared`'s link.
    pub fn new(shared: &'static UsbLinkShared) -> Self {
        Self {
            shared,
            packed: PackedLink::new(),
            inbox: VecDeque::new(),
            hello_owed: false,
        }
    }

    /// Take the link's events: client messages into the inbox, lifecycle
    /// into the transport's state. Stops at an `Up` (and takes nothing more
    /// while its hello is owed), so nothing of the new session is answered
    /// before the hello.
    fn pump_events(&mut self) {
        while !self.hello_owed {
            let Some(event) = self.shared.with_link(|link| link.recv()) else {
                return;
            };
            match event {
                LinkEvent::Message { channel, data } if channel == lp_link::CH_PROTO => {
                    if let Some(msg) = parse_request(&data) {
                        self.inbox.push_back(msg);
                    }
                }
                LinkEvent::Message { channel, data } => {
                    log::debug!("[usb_link] {} B on channel {channel} ignored", data.len());
                }
                LinkEvent::Text(text) => {
                    // A host speaks frames; text from it is a terminal
                    // someone typed into, or line noise.
                    log::debug!("[usb_link] {} B of host text ignored", text.len());
                }
                LinkEvent::Up { generation } => {
                    self.packed.back_to_json();
                    self.hello_owed = true;
                    log::info!("[usb_link] host link up (session {generation})");
                }
                LinkEvent::Reset { reason, generation } => {
                    self.packed.back_to_json();
                    // Requests from the ended session: its host has failed
                    // them itself (D9), and a reply would reach the next one.
                    self.inbox.clear();
                    usb_link_counters::note_reset(reason);
                    match reason {
                        ResetReason::PeerRestarted | ResetReason::Requested => log::info!(
                            "[usb_link] host link reset ({reason:?}); now session {generation}"
                        ),
                        ResetReason::RetryLimit | ResetReason::ProtocolError => log::warn!(
                            "[usb_link] host link reset ({reason:?}); now session {generation}"
                        ),
                    }
                }
            }
        }
    }

    /// Serialize `msg` and queue it: the learned table's step for it is
    /// rolled back if it is not queued. `wait`: wait for room in the send
    /// budget while the host drains.
    async fn write_once(
        &mut self,
        msg: &WireServerMessage,
        wait: bool,
    ) -> Result<Queued, TransportError> {
        let generation = self.shared.with_link(|link| link.generation());
        let tentative = self.packed.tentative();
        let len = serialize_server_payload(msg, self.packed.table_for(&msg.msg))?;
        let started = Instant::now();
        let queued = loop {
            let outcome = self.shared.with_link(|link| {
                if link.state() != LinkState::Established || link.generation() != generation {
                    return Queued::NoSession;
                }
                match link.send(lp_link::CH_PROTO, frame_bytes(len)) {
                    Ok(()) => Queued::Yes,
                    Err(SendError::Full) => Queued::Full,
                    Err(SendError::TooBig | SendError::BadChannel) => Queued::TooBig,
                }
            });
            match outcome {
                Queued::Full
                    if wait
                        && started.elapsed() < SEND_ROOM_WAIT
                        && !self.shared.with_link(|link| link.is_stalled(now_us())) =>
                {
                    // The host is draining: let the link task move frames,
                    // then look again. The payload stays in the frame
                    // buffer, which nothing else writes meanwhile.
                    self.shared.ring();
                    Timer::after(SEND_ROOM_POLL).await;
                }
                outcome => break outcome,
            }
        };
        match queued {
            Queued::Yes => self.shared.ring(),
            _ => self.packed.rolled_back(tentative),
        }
        Ok(queued)
    }
}

impl ServerTransport for UsbLinkTransport {
    async fn send(
        &mut self,
        _link: LinkId,
        mut msg: WireServerMessage,
    ) -> Result<(), TransportError> {
        self.pump_events();
        let id = msg.id;
        let is_hello = matches!(msg.msg, ServerMsgBody::Hello(_));
        if self.hello_owed && !is_hello {
            // Nothing of a new session goes before its hello (D5).
            usb_link_counters::note_reply_dropped_no_link();
            return Err(TransportError::ConnectionLost);
        }
        let is_error_frame = matches!(msg.msg, ServerMsgBody::Error { .. });
        // An opt-in answer `packed` needs a table first; with no heap for
        // one it becomes `json` here, before it is written. The answer
        // itself is always JSON (`table_for`), and the switch it announces
        // applies to every reply after it, until the session ends.
        self.packed.prepare_answer(&mut msg.msg);
        let switch_to = match msg.msg {
            ServerMsgBody::SetEncoding { encoding } => Some(encoding),
            _ => None,
        };
        let queued = match self.write_once(&msg, true).await {
            Ok(queued) => queued,
            Err(error) => {
                if switch_to.is_some() {
                    self.packed.answer_dropped();
                }
                log::error!("[usb_link] dropping message id={id}: {error}");
                return Err(error);
            }
        };
        match queued {
            Queued::Yes => {
                if is_hello {
                    self.hello_owed = false;
                }
                if let Some(encoding) = switch_to {
                    self.packed.answered(encoding);
                }
                Ok(())
            }
            Queued::NoSession => {
                if switch_to.is_some() {
                    self.packed.answer_dropped();
                }
                // Nobody to tell: no host, or its session ended while the
                // reply waited (and it failed the request itself, D9).
                usb_link_counters::note_reply_dropped_no_link();
                log::debug!("[usb_link] no host session; message id={id} dropped");
                Err(TransportError::ConnectionLost)
            }
            Queued::Full | Queued::TooBig => {
                if switch_to.is_some() {
                    self.packed.answer_dropped();
                }
                let error = if matches!(queued, Queued::Full) {
                    usb_link_counters::note_reply_dropped_full();
                    TransportError::Other(alloc::format!(
                        "host link send budget full ({} B held)",
                        self.shared.with_link(|link| link.buffered_bytes())
                    ))
                } else {
                    TransportError::Serialization(alloc::format!(
                        "message id={id} larger than the host link carries"
                    ))
                };
                // Final: say so at error level, never as a debuggable-away
                // warn (the "no silent drop" criterion).
                log::error!("[usb_link] dropping message id={id}: {error}");
                // Best effort: tell the peer its response was dropped, with
                // the request id, so the client fails that call instead of
                // timing out. Not for Error frames (no recursion), and without
                // waiting for room.
                if !is_error_frame {
                    let notice = WireServerMessage::new(
                        id,
                        ServerMsgBody::Error {
                            error: alloc::format!("response id={id} dropped: {error}"),
                        },
                    );
                    if !matches!(self.write_once(&notice, false).await, Ok(Queued::Yes)) {
                        log::error!(
                            "[usb_link] error notice for id={id} also dropped; peer left waiting"
                        );
                    }
                }
                Err(error)
            }
        }
    }

    async fn receive(&mut self) -> Result<Option<Incoming>, TransportError> {
        self.pump_events();
        // One link, and it is the USB cable: trusted.
        Ok(self.inbox.pop_front().map(Incoming::primary))
    }

    async fn receive_all(&mut self) -> Result<Vec<Incoming>, TransportError> {
        self.pump_events();
        Ok(self.inbox.drain(..).map(Incoming::primary).collect())
    }

    fn links(&self) -> Vec<Link> {
        alloc::vec![Link::PRIMARY]
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

/// The session's hello is owed once per `Up`; the server loop sends it at the
/// top of its next pass, before it takes anything else from the new session.
impl LinkUpkeep for UsbLinkTransport {
    fn take_opened_links(&mut self) -> Vec<Link> {
        self.pump_events();
        if core::mem::take(&mut self.hello_owed) {
            alloc::vec![Link::PRIMARY]
        } else {
            Vec::new()
        }
    }
}

/// One proto-channel message → the client message it carries, or `None`
/// (logged and counted: over a checked link a bad message is a host bug, not
/// line noise).
///
/// Out of line on purpose: the deserializer's frame stays its own, and never
/// joins the server loop future's.
#[inline(never)]
fn parse_request(data: &[u8]) -> Option<ClientMessage> {
    let Ok(text) = core::str::from_utf8(data) else {
        usb_link_counters::note_bad_request();
        log::warn!(
            "[usb_link] dropping a {} B proto message that is not UTF-8",
            data.len()
        );
        return None;
    };
    match lpc_wire::json::from_str::<ClientMessage>(text) {
        Ok(msg) => {
            log::debug!("[usb_link] received message id={}", msg.id);
            Some(msg)
        }
        Err(e) => {
            usb_link_counters::note_bad_request();
            let mut preview = text.len().min(48);
            while !text.is_char_boundary(preview) {
                preview -= 1;
            }
            log::warn!(
                "[usb_link] dropping a {} B proto message ({e}); prefix: {:?}",
                text.len(),
                &text[..preview]
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;
    use lp_link::{LinkConfig, Micros, SelectiveRepeat};
    use lpc_wire::WireEncoding;

    use crate::serial::server_msg::frame_buf_turn;

    #[test]
    fn a_session_opens_with_the_hello_and_carries_requests_and_replies() {
        let _turn = frame_buf_turn();
        let shared = UsbLinkShared::leak(0x1234_5678);
        let mut board = UsbLinkTransport::new(shared);
        let mut host = HostEnd::new(0x0bad_cafe);

        assert!(
            matches!(
                block(board.send(LinkId::PRIMARY, heartbeat())),
                Err(TransportError::ConnectionLost)
            ),
            "no host yet: nobody to send to"
        );

        host.pump(shared);
        assert!(host.saw_up());
        assert!(
            matches!(
                block(board.send(LinkId::PRIMARY, heartbeat())),
                Err(TransportError::ConnectionLost)
            ),
            "nothing of the session goes before its hello"
        );
        assert_eq!(board.take_opened_links(), vec![Link::PRIMARY]);
        assert!(board.take_opened_links().is_empty(), "owed once per Up");
        block(board.send(LinkId::PRIMARY, hello())).unwrap();
        host.pump(shared);
        let first = host.next_proto().expect("the hello");
        assert_eq!(first[0], b'{', "JSON until the host opts in");
        let first: WireServerMessage = lpc_wire::json::from_slice(&first).unwrap();
        assert!(matches!(first.msg, ServerMsgBody::Hello(_)));

        // A request arrives whole: no `M!`, no line.
        host.send_request(&ClientMessage {
            id: 7,
            msg: lpc_wire::ClientRequest::Hello,
        });
        host.pump(shared);
        let incoming = block(board.receive()).unwrap().expect("the request");
        assert_eq!(incoming.msg.id, 7);
        assert!(block(board.receive()).unwrap().is_none());

        // A reply goes back; so does a heartbeat now.
        block(board.send(LinkId::PRIMARY, error_reply(7))).unwrap();
        block(board.send(LinkId::PRIMARY, heartbeat())).unwrap();
        host.pump(shared);
        let reply: WireServerMessage =
            lpc_wire::json::from_slice(&host.next_proto().unwrap()).unwrap();
        assert_eq!(reply.id, 7);
        assert!(host.next_proto().is_some(), "the heartbeat");
    }

    #[test]
    fn a_packed_session_codes_against_a_table_and_a_new_session_is_json_again() {
        let _turn = frame_buf_turn();
        let shared = UsbLinkShared::leak(0x2345_6789);
        let mut board = UsbLinkTransport::new(shared);
        let mut host = HostEnd::new(0x1111_2222);
        open_session(shared, &mut board, &mut host);

        // The opt-in answer is JSON; everything after it is `L` + a learned
        // frame the host decodes against its own table.
        block(board.send(LinkId::PRIMARY, set_encoding(WireEncoding::Packed))).unwrap();
        host.pump(shared);
        assert_eq!(host.next_proto().unwrap()[0], b'{');
        let mut table = lp_json_pack::LearnedTable::NEW;
        for n in 0..5 {
            block(board.send(LinkId::PRIMARY, log_reply(n))).unwrap();
            host.pump(shared);
            let payload = host.next_proto().unwrap();
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

        // The host restarts (a reload): the board resets with it, the new
        // session is owed a hello, and its replies are JSON again.
        let resets_before = usb_link_counters::edge().resets_peer_restarted;
        let mut host = HostEnd::new(0x3333_4444);
        host.pump(shared);
        assert!(host.saw_up());
        assert_eq!(board.take_opened_links(), vec![Link::PRIMARY]);
        assert!(usb_link_counters::edge().resets_peer_restarted > resets_before);
        block(board.send(LinkId::PRIMARY, hello())).unwrap();
        block(board.send(LinkId::PRIMARY, log_reply(9))).unwrap();
        host.pump(shared);
        assert_eq!(host.next_proto().unwrap()[0], b'{', "the hello");
        assert_eq!(host.next_proto().unwrap()[0], b'{', "JSON again");
    }

    #[test]
    fn a_request_that_is_not_a_client_message_is_counted_and_dropped() {
        let _turn = frame_buf_turn();
        let shared = UsbLinkShared::leak(0x4567_89ab);
        let mut board = UsbLinkTransport::new(shared);
        let mut host = HostEnd::new(0x5555_6666);
        open_session(shared, &mut board, &mut host);
        let before = usb_link_counters::edge().bad_requests;
        host.link.send(lp_link::CH_PROTO, b"M!{not json").unwrap();
        host.pump(shared);
        assert!(block(board.receive()).unwrap().is_none());
        assert!(usb_link_counters::edge().bad_requests > before);
    }

    /// Bring a session up and deliver its hello.
    fn open_session(shared: &UsbLinkShared, board: &mut UsbLinkTransport, host: &mut HostEnd) {
        host.pump(shared);
        assert!(host.saw_up());
        assert_eq!(board.take_opened_links(), vec![Link::PRIMARY]);
        block(board.send(LinkId::PRIMARY, hello())).unwrap();
        host.pump(shared);
        assert!(host.next_proto().is_some(), "the hello");
    }

    /// The host's end of the link, and the wire between it and the board's,
    /// on a clock of its own (far ahead of the embassy clock the transport
    /// reads, so the board never looks stalled).
    struct HostEnd {
        link: lp_link::Link<SelectiveRepeat>,
        events: VecDeque<LinkEvent>,
        now: Micros,
    }

    impl HostEnd {
        fn new(nonce: u32) -> Self {
            Self {
                link: lp_link::Link::new(LinkConfig::usb(), nonce),
                events: VecDeque::new(),
                now: 1_000_000_000_000,
            }
        }

        /// Move frames both ways, 2 ms of link time a round, until the wire
        /// has been quiet for a few rounds (delayed ACKs included).
        fn pump(&mut self, shared: &UsbLinkShared) {
            let mut quiet = 0;
            for _ in 0..500 {
                self.now += 2_000;
                let t = self.now;
                let mut moved = false;
                while let Some(frame) = shared.with_link(|l| l.poll_transmit(t).map(<[u8]>::to_vec))
                {
                    self.link.on_bytes(t, &frame);
                    moved = true;
                }
                while let Some(frame) = self.link.poll_transmit(t).map(<[u8]>::to_vec) {
                    shared.with_link(|l| l.on_bytes(t, &frame));
                    moved = true;
                }
                while let Some(event) = self.link.recv() {
                    self.events.push_back(event);
                }
                quiet = if moved { 0 } else { quiet + 1 };
                if quiet >= 5 {
                    break;
                }
            }
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

        /// The next proto-channel message, skipping log records.
        fn next_proto(&mut self) -> Option<Vec<u8>> {
            while let Some(event) = self.events.pop_front() {
                if let LinkEvent::Message { channel, data } = event
                    && channel == lp_link::CH_PROTO
                {
                    return Some(data);
                }
            }
            None
        }

        fn send_request(&mut self, msg: &ClientMessage) {
            let json = lpc_wire::json::to_string(msg).unwrap();
            self.link.send(lp_link::CH_PROTO, json.as_bytes()).unwrap();
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

    /// Any small unsolicited frame stands in for a heartbeat here.
    fn heartbeat() -> WireServerMessage {
        WireServerMessage::new(0, ServerMsgBody::StopAllProjects)
    }

    fn error_reply(id: u64) -> WireServerMessage {
        WireServerMessage::new(
            id,
            ServerMsgBody::Error {
                error: "x".to_string(),
            },
        )
    }

    fn log_reply(n: u64) -> WireServerMessage {
        WireServerMessage::new(
            n,
            ServerMsgBody::Log {
                level: lpc_wire::server::api::LogLevel::Info,
                message: alloc::format!("line {n}"),
            },
        )
    }

    fn set_encoding(encoding: WireEncoding) -> WireServerMessage {
        WireServerMessage::new(
            lpc_wire::PACK_OPT_IN_REQUEST_ID,
            ServerMsgBody::SetEncoding { encoding },
        )
    }

    fn block<F: core::future::Future>(future: F) -> F::Output {
        embassy_futures::block_on(future)
    }
}
