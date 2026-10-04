//! A host's end of one device link: lp-link underneath, wire messages,
//! console lines and link events on top.
//!
//! Since `WIRE_PROTO_VERSION` 30 a board's USB serial link is an lp-link
//! (plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`): frames with a
//! checksum, resent when lost, on channels, with a lifecycle both ends share.
//! [`WireLinkPort`] is what every host keeps **one of per port**, for the
//! port's whole life (Studio's Web Serial provider and emulator tab, lp-cli's
//! serial and emulated-board transports, the fake board's host side). It
//! replaces the `M!`-era `WireStream` + `PackOptIn` pair for those links:
//!
//! - bytes from the port go in ([`on_bytes`](WireLinkPort::on_bytes)), frames
//!   to write come out ([`poll_transmit`](WireLinkPort::poll_transmit)), and
//!   the caller wakes it at [`poll_timeout`](WireLinkPort::poll_timeout);
//! - requests go in as one proto-channel message each
//!   ([`send_client`](WireLinkPort::send_client));
//! - what the board said comes out as [`PortRead`]s: wire messages (decoded
//!   once, JSON or packed — [`ServerPayload`]), console lines (log records
//!   from the log channel and raw text outside frames, one line each), and
//!   the link's own `Up`/`Reset`.
//!
//! **Per-link state resets with the link.** On every `Up` and `Reset` the
//! learned table and the packed opt-in start over, on both ends. The board
//! sends its hello first after every `Up` (D5); when the hello offers this
//! build's [`PACK_FORMAT_VERSION`], a port that wants packed replies asks
//! once ([`PACK_OPT_IN_REQUEST_ID`]) and swallows the answer, as the `M!`
//! reader did. There is no re-ask timer and no desync path: over a reliable
//! link one ask per session is enough, and a packed payload that does not
//! decode is a bug — counted ([`LinkCounters::payload_errors`]), noted, and
//! answered by restarting the link, which resets both tables.
//!
//! A `Reset` is surfaced at once ([`PortRead::Reset`]) so request/response
//! layers above fail what was in flight instead of waiting out idle budgets
//! (D9).
//!
//! Sans-IO: time is the caller's ([`Micros`], any monotonic microsecond
//! count), and so is the nonce (random per port open).
//!
//! **Secure ports** (feature `secure-link`): [`WireLinkPort::new_secure`]
//! builds the port as a secure lp-link initiator holding a key (Noise
//! NNpsk0 inside the SYN, then sealed frames). What the handshake says beyond
//! `Up` (a refusal, a peer that is not secure) comes out of
//! [`WireLinkPort::poll_secure_event`], with a [`PortRead::Note`] line for a
//! journal; [`WireLinkPort::retry_with`] tries another key. No variant of
//! [`PortRead`] depends on the feature, so a build that turns it on breaks no
//! match elsewhere.

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::format;
use alloc::string::{String, ToString};

use crate::console_line::{TextLines, log_record_lines};
use lp_json_pack::{LearnStore, LearnedTable, PACK_FORMAT_VERSION};
use lp_link::{
    CH_LOG, CH_PROTO, Link, LinkConfig, LinkEvent, LinkState, Micros, ResetReason, SelectiveRepeat,
    SendError,
};

use crate::link_counter_tally::LinkCounterTally;
use crate::link_payload::{ServerPayload, decode_server_payload, encode_client_payload};
use crate::message::client::{ClientMessage, ClientRequest};
use crate::server::api::LogLevel;
use crate::server::{LinkCounters, ServerMsgBody};
use crate::{PACK_OPT_IN_REQUEST_ID, WIRE_PROTO_VERSION, WireEncoding, WireServerMessage};

/// The id the dev log-level request goes out with: one below the opt-in's,
/// so it is as far from any request counter and still exact in JavaScript.
pub const DEVICE_LOG_LEVEL_REQUEST_ID: u64 = PACK_OPT_IN_REQUEST_ID - 1;

/// One thing a [`WireLinkPort`] read, in the order the board said it.
#[derive(Debug)]
pub enum PortRead {
    /// One wire message off the proto channel.
    Message(ServerPayload),
    /// One console line: a log record from the log channel
    /// (`[LEVEL] target: text`, as the board's logger always printed it), or
    /// a line of raw text outside frames (boot text, a panic, the boot
    /// marker). No newline.
    Log(String),
    /// The link came up (a new session): the board's hello follows.
    Up { generation: u32 },
    /// The session ended. Every request in flight on it is lost: fail it now.
    Reset { reason: ResetReason },
    /// Something about the link worth one line in a device journal (the
    /// packed opt-in's outcome, a proto mismatch, a payload that did not
    /// decode). At most one per change.
    Note(String),
}

/// Where this session's packed opt-in stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OptIn {
    /// No hello yet this session.
    WaitingForHello,
    /// Asked; the answer has not come.
    Asked,
    /// The board packs its replies on this session.
    Packed,
    /// Settled on JSON (not wanted, not offered, or refused).
    Json,
}

/// A host's end of one device link. See the module docs.
pub struct WireLinkPort {
    link: Link<SelectiveRepeat>,
    table: Box<LearnedTable>,
    want_packed: bool,
    device_log: Option<LogLevel>,
    opt_in: OptIn,
    device_log_sent: bool,
    reads: VecDeque<PortRead>,
    /// Raw text since the last newline.
    text: TextLines,
    tally: LinkCounterTally,
    now: Micros,
    /// A secure port's handshake events, for `poll_secure_event`.
    #[cfg(feature = "secure-link")]
    secure_events: VecDeque<lp_link::secure_channel::SecureEvent>,
}

impl WireLinkPort {
    /// A port on a link tuned by `config`, the transport's preset
    /// ([`LinkConfig::usb`] for a USB-Serial-JTAG board, [`LinkConfig::uart`]
    /// for a UART behind a USB-serial bridge). `nonce` must be random per
    /// port open; `want_packed` asks boards to pack their replies (Studio's
    /// `?wire=json` and `LP_WIRE_ENCODING=json` say no).
    pub fn new(config: LinkConfig, nonce: u32, want_packed: bool) -> Self {
        WireLinkPort {
            link: Link::new(config, nonce),
            table: LearnedTable::boxed(),
            want_packed,
            device_log: None,
            opt_in: OptIn::WaitingForHello,
            device_log_sent: false,
            reads: VecDeque::new(),
            text: TextLines::new(),
            tally: LinkCounterTally::new(),
            now: 0,
            #[cfg(feature = "secure-link")]
            secure_events: VecDeque::new(),
        }
    }

    /// A port that is a secure lp-link initiator (feature `secure-link`):
    /// it names `key_id` (an access entry's salt) in the clear and proves it
    /// holds `psk` (`lpc_access::link_psk` of the entry's key). `entropy`
    /// fills a buffer with fresh random bytes (32 per handshake). The device
    /// grants that entry's tier, and its hello says so.
    #[cfg(feature = "secure-link")]
    pub fn new_secure(
        config: LinkConfig,
        nonce: u32,
        want_packed: bool,
        key_id: lp_link::secure_channel::KeyId,
        psk: lp_link::secure_channel::Psk,
        entropy: fn(&mut [u8]),
    ) -> Self {
        let mut port = Self::new(config.clone(), nonce, want_packed);
        port.link = Link::new_secure(
            config,
            nonce,
            lp_link::secure_channel::SecureRole::Initiator { key_id, psk },
            entropy,
        );
        port
    }

    /// The next thing a secure port's handshake said beyond `Up`: a refusal
    /// (answer it with [`retry_with`](Self::retry_with) or give up) or a
    /// peer that is not secure.
    #[cfg(feature = "secure-link")]
    pub fn poll_secure_event(&mut self) -> Option<lp_link::secure_channel::SecureEvent> {
        self.pump_events();
        self.secure_events.pop_front()
    }

    /// A secure port, after a refusal: try another key. The handshake starts
    /// again at once.
    #[cfg(feature = "secure-link")]
    pub fn retry_with(
        &mut self,
        key_id: lp_link::secure_channel::KeyId,
        psk: lp_link::secure_channel::Psk,
    ) {
        self.link.retry_with(key_id, psk);
    }

    /// Dev-only (Studio's `?device-log=<level>`): also ask the board for
    /// `level` once per session, after its hello and the opt-in, and swallow
    /// the answer into a [`PortRead::Note`]. `None` never asks.
    pub fn with_device_log_level(mut self, level: Option<LogLevel>) -> Self {
        self.device_log = level;
        self
    }

    // ---- Transport side --------------------------------------------------

    /// Bytes read from the port at `now` (any split).
    pub fn on_bytes(&mut self, now: Micros, bytes: &[u8]) {
        self.now = now;
        self.link.on_bytes(now, bytes);
        self.note_stall(now);
        self.pump_events();
    }

    /// The next frame to write, if any: call until `None` whenever the port
    /// can take more, and after every [`send_client`](Self::send_client).
    pub fn poll_transmit(&mut self, now: Micros) -> Option<&[u8]> {
        self.now = now;
        self.note_stall(now);
        self.link.poll_transmit(now)
    }

    /// When the port next needs [`poll_transmit`](Self::poll_transmit) for a
    /// timer (resend, acknowledgement, keepalive, handshake).
    pub fn poll_timeout(&self) -> Option<Micros> {
        self.link.poll_timeout()
    }

    // ---- Application side ------------------------------------------------

    /// Queue a request (as one JSON proto message).
    pub fn send_client(&mut self, message: &ClientMessage) -> Result<(), SendError> {
        self.link.send(CH_PROTO, &encode_client_payload(message))
    }

    /// Queue a request already serialized as JSON (no `M!`, no newline):
    /// for conversation layers that carry requests as text.
    pub fn send_client_json(&mut self, json: &str) -> Result<(), SendError> {
        self.link.send(CH_PROTO, json.as_bytes())
    }

    /// The next thing the board said, in order.
    pub fn poll_read(&mut self) -> Option<PortRead> {
        self.pump_events();
        self.reads.pop_front()
    }

    /// Drop the session and start a new one (both ends reset).
    pub fn restart(&mut self, now: Micros) {
        self.now = now;
        self.link.restart(now);
        self.pump_events();
    }

    // ---- Introspection ---------------------------------------------------

    /// Connecting (handshaking) or established.
    pub fn state(&self) -> LinkState {
        self.link.state()
    }

    /// Up, but the board has been silent past the link's stall time (a
    /// cable out, a hung board). The session is kept.
    pub fn is_stalled(&self, now: Micros) -> bool {
        self.link.is_stalled(now)
    }

    /// This end's counters (lp-link's, with resets by reason, stalls and
    /// payload errors).
    pub fn counters(&self) -> LinkCounters {
        self.tally.snapshot(self.link.counters())
    }

    /// The encoding the board is writing this session's replies in.
    pub fn encoding(&self) -> WireEncoding {
        match self.opt_in {
            OptIn::Packed => WireEncoding::Packed,
            _ => WireEncoding::Json,
        }
    }

    /// The link itself, for whatever else a caller wants to know.
    pub fn link(&self) -> &Link<SelectiveRepeat> {
        &self.link
    }

    // ---- Internals -------------------------------------------------------

    fn note_stall(&mut self, now: Micros) {
        self.tally.note_stalled(self.link.is_stalled(now));
    }

    /// Move everything the link has for us into `reads`, acting on it.
    fn pump_events(&mut self) {
        #[cfg(feature = "secure-link")]
        self.pump_secure_events();
        while let Some(event) = self.link.recv() {
            self.tally.note_event(&event);
            match event {
                LinkEvent::Up { generation } => {
                    self.forget_session();
                    self.reads.push_back(PortRead::Up { generation });
                }
                LinkEvent::Reset { reason, .. } => {
                    self.forget_session();
                    self.reads.push_back(PortRead::Reset { reason });
                }
                LinkEvent::Message { channel, data } => match channel {
                    CH_PROTO => self.on_proto(&data),
                    CH_LOG => self.on_log(&data),
                    // Channel 0 and the rest are unused on a device link.
                    _ => {}
                },
                LinkEvent::Text(bytes) => self.on_text(&bytes),
            }
        }
    }

    /// A secure port's handshake events: kept for `poll_secure_event`, and
    /// one journal line each.
    #[cfg(feature = "secure-link")]
    fn pump_secure_events(&mut self) {
        use lp_link::secure_channel::SecureEvent;
        while let Some(event) = self.link.poll_secure_event() {
            let note = match event {
                SecureEvent::Refused {
                    reason,
                    retry_after_ms,
                } => format!(
                    "link: the device refused this key ({reason:?}, retry after {retry_after_ms} ms)"
                ),
                SecureEvent::PeerNotSecure => {
                    "link: the device runs a plain link; a secure port will not come up".to_string()
                }
                // Responder events never reach an initiator.
                SecureEvent::KeyLookup { .. } | SecureEvent::WrongKey { .. } => continue,
            };
            self.reads.push_back(PortRead::Note(note));
            self.secure_events.push_back(event);
        }
    }

    fn forget_session(&mut self) {
        self.table.reset(0);
        self.opt_in = OptIn::WaitingForHello;
        self.device_log_sent = false;
    }

    fn on_proto(&mut self, data: &[u8]) {
        let payload = match decode_server_payload(data, &mut *self.table) {
            Ok(payload) => payload,
            Err(error) => {
                // A bug over a reliable link: say so, count it, and start
                // both ends over (the tables reset with the session).
                self.tally.note_payload_error();
                self.reads.push_back(PortRead::Note(format!(
                    "link: a reply did not decode ({error}); restarting the link"
                )));
                self.link.restart(self.now);
                return;
            }
        };
        if let Ok(message) = &payload.message {
            if self.swallow_own_answer(message) {
                return;
            }
            if let ServerMsgBody::Hello(hello) = &message.msg {
                self.on_hello(hello.proto, hello.pack_format);
            }
        }
        self.reads.push_back(PortRead::Message(payload));
        self.maybe_ask_log_level();
    }

    fn on_hello(&mut self, proto: u32, pack_format: u8) {
        if self.opt_in != OptIn::WaitingForHello {
            return;
        }
        if !self.want_packed {
            self.opt_in = OptIn::Json;
            return;
        }
        let stays_json = if proto != WIRE_PROTO_VERSION {
            Some(format!(
                "wire: replies stay JSON — the board speaks proto {proto}, this build \
                 {WIRE_PROTO_VERSION}"
            ))
        } else if pack_format == 0 {
            Some(
                "wire: replies stay JSON — the board does not pack (its hello offers no pack \
                 format)"
                    .to_string(),
            )
        } else if pack_format != PACK_FORMAT_VERSION {
            Some(format!(
                "wire: replies stay JSON — the board packs JSON Pack v{pack_format}, this build \
                 v{PACK_FORMAT_VERSION}"
            ))
        } else {
            None
        };
        match stays_json {
            Some(note) => {
                self.opt_in = OptIn::Json;
                self.reads.push_back(PortRead::Note(note));
            }
            None => {
                let ask = ClientMessage {
                    id: PACK_OPT_IN_REQUEST_ID,
                    msg: ClientRequest::SetEncoding {
                        encoding: WireEncoding::Packed,
                        format: PACK_FORMAT_VERSION,
                    },
                };
                self.opt_in = match self.send_client(&ask) {
                    Ok(()) => OptIn::Asked,
                    Err(_) => OptIn::Json,
                };
            }
        }
    }

    /// The answers to this port's own requests (the opt-in, the dev log
    /// level) are this port's: noted, never handed on.
    fn swallow_own_answer(&mut self, message: &WireServerMessage) -> bool {
        match message.id {
            PACK_OPT_IN_REQUEST_ID => {
                let note = match &message.msg {
                    ServerMsgBody::SetEncoding {
                        encoding: WireEncoding::Packed,
                    } => {
                        self.opt_in = OptIn::Packed;
                        format!("wire: replies packed (JSON Pack v{PACK_FORMAT_VERSION}, learned)")
                    }
                    ServerMsgBody::SetEncoding {
                        encoding: WireEncoding::Json,
                    } => {
                        self.opt_in = OptIn::Json;
                        "wire: replies stay JSON — the board answered the opt-in with `json`"
                            .to_string()
                    }
                    ServerMsgBody::Error { error } => {
                        self.opt_in = OptIn::Json;
                        format!("wire: replies stay JSON — the board refused the opt-in: {error}")
                    }
                    _ => return false,
                };
                self.reads.push_back(PortRead::Note(note));
                self.maybe_ask_log_level();
                true
            }
            DEVICE_LOG_LEVEL_REQUEST_ID => {
                let note = match &message.msg {
                    ServerMsgBody::SetLogLevel => {
                        "dev: the board applied the requested log level".to_string()
                    }
                    ServerMsgBody::Error { error } => {
                        format!("dev: the board refused the requested log level: {error}")
                    }
                    _ => return false,
                };
                self.reads.push_back(PortRead::Note(note));
                true
            }
            _ => false,
        }
    }

    /// Ask for the dev log level once the hello is in and the opt-in has
    /// settled (or was never asked).
    fn maybe_ask_log_level(&mut self) {
        let Some(level) = self.device_log else {
            return;
        };
        if self.device_log_sent || matches!(self.opt_in, OptIn::WaitingForHello | OptIn::Asked) {
            return;
        }
        self.device_log_sent = true;
        let _ = self.send_client(&ClientMessage {
            id: DEVICE_LOG_LEVEL_REQUEST_ID,
            msg: ClientRequest::SetLogLevel { level },
        });
    }

    fn on_log(&mut self, record: &[u8]) {
        for line in log_record_lines(record) {
            self.reads.push_back(PortRead::Log(line));
        }
    }

    fn on_text(&mut self, bytes: &[u8]) {
        let reads = &mut self.reads;
        self.text
            .push(bytes, |line| reads.push_back(PortRead::Log(line)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::hello::{BuildFacts, HardwareFacts, ServerHello};
    use alloc::vec;
    use alloc::vec::Vec;

    #[test]
    fn the_handshake_brings_the_link_up_and_the_hello_through() {
        let mut t = Bench::new(false);
        t.run(50);
        let reads = t.reads();
        assert!(matches!(reads[0], PortRead::Up { .. }), "{reads:?}");
        let hello = messages(&reads);
        assert_eq!(hello.len(), 1);
        assert!(hello[0].json.contains("\"hello\""));
        assert!(!hello[0].packed);
        assert!(t.board.asks.is_empty(), "a port that wants JSON never asks");
        assert_eq!(t.port.encoding(), WireEncoding::Json);
    }

    #[test]
    fn requests_reach_the_board_and_replies_come_back() {
        let mut t = Bench::new(false);
        t.run(50);
        t.reads();
        t.port
            .send_client(&ClientMessage {
                id: 7,
                msg: ClientRequest::Hello,
            })
            .unwrap();
        t.run(50);
        assert_eq!(t.board.requests, vec![7]);
        let reads = t.reads();
        let replies = messages(&reads);
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0].message.as_ref().unwrap().id, 7);
    }

    #[test]
    fn a_port_on_the_uart_preset_carries_the_hello_and_a_request() {
        let mut t = Bench::on(LinkConfig::uart(), false);
        t.run(50);
        let reads = t.reads();
        assert!(matches!(reads[0], PortRead::Up { .. }), "{reads:?}");
        assert!(messages(&reads)[0].json.contains("\"hello\""));
        t.port
            .send_client(&ClientMessage {
                id: 7,
                msg: ClientRequest::Hello,
            })
            .unwrap();
        t.run(50);
        assert_eq!(t.board.requests, vec![7]);
        assert_eq!(messages(&t.reads()).len(), 1);
    }

    #[test]
    fn a_port_on_the_uart_preset_queues_an_upload_sized_request() {
        // The host queues each request with `send()`, bounded by
        // `send_budget`: an upload's ~5.5 KB chunk must be taken.
        let mut t = Bench::on(LinkConfig::uart(), false);
        t.run(50);
        let chunk = "x".repeat(6 * 1024);
        assert_eq!(t.port.send_client_json(&chunk), Ok(()));
    }

    #[test]
    fn a_board_that_offers_another_format_stays_json_and_says_so_once() {
        let mut t = Bench::new(true);
        t.board.pack_format = PACK_FORMAT_VERSION + 1;
        t.run(50);
        let reads = t.reads();
        let notes = notes(&reads);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("stay JSON"), "{notes:?}");
        assert!(t.board.asks.is_empty());
    }

    #[cfg(feature = "ser-write-json")]
    #[test]
    fn hello_then_opt_in_then_packed_replies_decode() {
        let mut t = Bench::new(true);
        t.run(50);
        assert_eq!(t.board.asks, vec![PACK_OPT_IN_REQUEST_ID]);
        let reads = t.reads();
        assert!(
            notes(&reads).iter().any(|n| n.contains("replies packed")),
            "{reads:?}"
        );
        assert!(
            messages(&reads)
                .iter()
                .all(|m| m.message.as_ref().unwrap().id != PACK_OPT_IN_REQUEST_ID),
            "the opt-in's answer is swallowed"
        );
        assert_eq!(t.port.encoding(), WireEncoding::Packed);
        // Many replies, so the learned table has something to learn.
        for id in 100..140 {
            t.board.reply(id);
        }
        t.run(200);
        let reads = t.reads();
        let replies = messages(&reads);
        assert_eq!(replies.len(), 40);
        assert!(replies.iter().all(|m| m.packed));
        let ids: Vec<u64> = replies
            .iter()
            .map(|m| m.message.as_ref().unwrap().id)
            .collect();
        assert_eq!(ids, (100..140).collect::<Vec<_>>());
        assert_eq!(t.port.counters().payload_errors, 0);
    }

    #[cfg(feature = "ser-write-json")]
    #[test]
    fn a_board_reboot_resets_the_table_and_asks_again() {
        let mut t = Bench::new(true);
        t.run(50);
        for id in 1..10 {
            t.board.reply(id);
        }
        t.run(100);
        t.reads();
        // The board reboots: a new link, a new nonce, an empty table.
        t.board.reboot(0xBEEF_0002);
        t.run(300);
        let reads = t.reads();
        let reset_at = reads
            .iter()
            .position(|r| matches!(r, PortRead::Reset { .. }))
            .expect("the reset is surfaced");
        assert!(matches!(
            reads[reset_at],
            PortRead::Reset {
                reason: ResetReason::PeerRestarted
            }
        ));
        assert!(
            reads[reset_at..]
                .iter()
                .any(|r| matches!(r, PortRead::Up { .. }))
        );
        assert_eq!(t.board.asks.len(), 2, "asked once per session");
        for id in 20..30 {
            t.board.reply(id);
        }
        t.run(100);
        let reads = t.reads();
        let replies = messages(&reads);
        assert_eq!(replies.len(), 10);
        assert!(replies.iter().all(|m| m.packed && m.message.is_ok()));
        let counters = t.port.counters();
        assert_eq!(counters.resets.peer_restarted, 1);
        assert_eq!(counters.payload_errors, 0);
    }

    #[test]
    fn a_board_reboot_is_one_reset_surfaced_at_once_then_up_and_hello() {
        let mut t = Bench::new(false);
        t.run(50);
        t.reads();
        t.board.reboot(0xBEEF_0002);
        t.run(300);
        let reads = t.reads();
        let kinds: Vec<&str> = reads
            .iter()
            .map(|r| match r {
                PortRead::Reset { .. } => "reset",
                PortRead::Up { .. } => "up",
                PortRead::Message(_) => "message",
                PortRead::Log(_) => "log",
                PortRead::Note(_) => "note",
            })
            .collect();
        assert_eq!(kinds, vec!["reset", "up", "message"], "{reads:?}");
        assert_eq!(t.port.counters().resets.peer_restarted, 1);
    }

    #[test]
    fn log_records_and_raw_text_become_lines() {
        let mut t = Bench::new(false);
        t.run(50);
        t.reads();
        t.board.link.send(CH_LOG, b"\x03main: tick 1").unwrap();
        t.board.link.send(CH_LOG, b"\x01fs: gone\r\n").unwrap();
        t.board
            .link
            .send(CH_LOG, b"\x003 log records dropped")
            .unwrap();
        t.run(20);
        t.port
            .on_bytes(t.now, b"[INIT] fw-esp32 initialized, proto=30\r\npart");
        t.port.on_bytes(t.now, b"ial line\n");
        let logs: Vec<String> = t
            .reads()
            .into_iter()
            .filter_map(|r| match r {
                PortRead::Log(line) => Some(line),
                _ => None,
            })
            .collect();
        assert_eq!(
            logs,
            vec![
                "[INFO] main: tick 1",
                "[ERROR] fs: gone",
                "[LINK] 3 log records dropped",
                "[INIT] fw-esp32 initialized, proto=30",
                "partial line",
            ]
        );
    }

    #[test]
    fn a_payload_that_does_not_decode_is_counted_and_restarts_the_link() {
        let mut t = Bench::new(false);
        t.run(50);
        t.reads();
        t.board
            .link
            .send(CH_PROTO, b"Lnot a learned frame")
            .unwrap();
        t.run(100);
        let reads = t.reads();
        assert!(
            notes(&reads).iter().any(|n| n.contains("did not decode")),
            "{reads:?}"
        );
        assert!(reads.iter().any(|r| matches!(
            r,
            PortRead::Reset {
                reason: ResetReason::Requested
            }
        )));
        let counters = t.port.counters();
        assert_eq!(counters.payload_errors, 1);
        assert_eq!(counters.resets.requested, 1);
        // …and the link comes back.
        assert!(reads.iter().any(|r| matches!(r, PortRead::Up { .. })));
        assert_eq!(t.port.state(), LinkState::Established);
    }

    #[test]
    fn the_dev_log_level_is_asked_once_after_the_hello_and_swallowed() {
        let mut t = Bench::new(false);
        t.port = WireLinkPort::new(LinkConfig::usb(), 0xAAAA_0001, false)
            .with_device_log_level(Some(LogLevel::Debug));
        t.run(80);
        assert_eq!(t.board.asks, vec![DEVICE_LOG_LEVEL_REQUEST_ID]);
        let reads = t.reads();
        assert!(
            notes(&reads)
                .iter()
                .any(|n| n.contains("applied the requested log level")),
            "{reads:?}"
        );
        assert_eq!(messages(&reads).len(), 1, "only the hello is handed on");
    }

    #[test]
    fn a_silent_board_is_a_stall_counted_once() {
        let mut t = Bench::new(false);
        t.run(50);
        let start = t.now;
        // Nothing reaches the port for two seconds.
        for step in 0..2_000 {
            let now = start + step * 1_000;
            while t.port.poll_transmit(now).is_some() {}
        }
        assert!(t.port.is_stalled(start + 2_000_000));
        assert_eq!(t.port.counters().stalls, 1);
    }

    /// A secure port against a secure board: up through the handshake,
    /// the hello read through sealed frames, requests answered.
    #[cfg(feature = "secure-link")]
    #[test]
    fn a_secure_port_comes_up_and_reads_the_board_through_sealed_frames() {
        let mut t = Bench::secure(secure::KEY_ID);
        t.run(50);
        let reads = t.reads();
        assert!(matches!(reads[0], PortRead::Up { .. }), "{reads:?}");
        assert_eq!(messages(&reads).len(), 1, "the hello");
        assert!(t.board.link.session_auth().is_some());
        t.port
            .send_client(&ClientMessage {
                id: 7,
                msg: ClientRequest::LoginBegin,
            })
            .unwrap();
        t.run(20);
        assert_eq!(t.board.requests, vec![7]);
        assert_eq!(messages(&t.reads()).len(), 1);
        assert_eq!(t.port.poll_secure_event(), None);
    }

    /// A refused key comes out as an event and one note line; another key
    /// brings the port up.
    #[cfg(feature = "secure-link")]
    #[test]
    fn a_refused_key_is_an_event_and_a_note_and_another_key_comes_up() {
        use lp_link::secure_channel::{KeyId, RefusalReason, SecureEvent};
        let mut t = Bench::secure(KeyId([9; 16]));
        t.run(50);
        assert_eq!(
            t.port.poll_secure_event(),
            Some(SecureEvent::Refused {
                reason: RefusalReason::UnknownKey,
                retry_after_ms: 0
            })
        );
        let reads = t.reads();
        assert!(
            notes(&reads).iter().any(|n| n.contains("refused this key")),
            "{reads:?}"
        );
        assert_ne!(t.port.state(), LinkState::Established);
        t.port.retry_with(secure::KEY_ID, secure::psk());
        t.run(50);
        assert_eq!(t.port.state(), LinkState::Established);
    }

    /// A secure port and a plain board: never up, said once.
    #[cfg(feature = "secure-link")]
    #[test]
    fn a_secure_port_and_a_plain_board_never_come_up() {
        let mut t = Bench::secure(secure::KEY_ID);
        t.board = BoardDouble::new(0xBEEF_0001);
        t.run(300);
        assert_ne!(t.port.state(), LinkState::Established);
        assert_eq!(
            t.port.poll_secure_event(),
            Some(lp_link::secure_channel::SecureEvent::PeerNotSecure)
        );
        assert_eq!(t.port.poll_secure_event(), None);
    }

    fn messages(reads: &[PortRead]) -> Vec<&ServerPayload> {
        reads
            .iter()
            .filter_map(|r| match r {
                PortRead::Message(m) => Some(m),
                _ => None,
            })
            .collect()
    }

    fn notes(reads: &[PortRead]) -> Vec<&str> {
        reads
            .iter()
            .filter_map(|r| match r {
                PortRead::Note(n) => Some(n.as_str()),
                _ => None,
            })
            .collect()
    }

    /// The port wired to a board double over a lossless pipe.
    struct Bench {
        port: WireLinkPort,
        board: BoardDouble,
        now: Micros,
        reads: Vec<PortRead>,
    }

    impl Bench {
        fn new(want_packed: bool) -> Self {
            Self::on(LinkConfig::usb(), want_packed)
        }

        /// Both ends on `config`.
        fn on(config: LinkConfig, want_packed: bool) -> Self {
            Bench {
                port: WireLinkPort::new(config.clone(), 0xAAAA_0001, want_packed),
                board: BoardDouble::on(config, 0xBEEF_0001),
                now: 0,
                reads: Vec::new(),
            }
        }

        /// A secure port holding `key_id` against a secure board that knows
        /// only [`secure::KEY_ID`].
        #[cfg(feature = "secure-link")]
        fn secure(key_id: lp_link::secure_channel::KeyId) -> Self {
            let mut bench = Self::new(false);
            bench.port = WireLinkPort::new_secure(
                LinkConfig::usb(),
                0xAAAA_0001,
                false,
                key_id,
                secure::psk(),
                secure::entropy,
            );
            bench.board.link = Link::new_secure(
                LinkConfig::usb(),
                0xBEEF_0001,
                lp_link::secure_channel::SecureRole::Responder,
                secure::entropy,
            );
            bench
        }

        /// `steps` milliseconds of both ends running.
        fn run(&mut self, steps: u64) {
            for _ in 0..steps {
                while let Some(frame) = self.port.poll_transmit(self.now) {
                    let frame = frame.to_vec();
                    self.board.link.on_bytes(self.now, &frame);
                }
                self.board.serve();
                while let Some(frame) = self.board.link.poll_transmit(self.now) {
                    let frame = frame.to_vec();
                    self.port.on_bytes(self.now, &frame);
                }
                while let Some(read) = self.port.poll_read() {
                    self.reads.push(read);
                }
                self.now += 1_000;
            }
        }

        fn reads(&mut self) -> Vec<PortRead> {
            while let Some(read) = self.port.poll_read() {
                self.reads.push(read);
            }
            core::mem::take(&mut self.reads)
        }
    }

    /// A board's end, as the firmware will run it: hello on every `Up`,
    /// JSON until the host opts in, a fresh table per session.
    struct BoardDouble {
        link: Link<SelectiveRepeat>,
        table: LearnedTable,
        packed: bool,
        pack_format: u8,
        /// Ids of requests answered by the double's own rules (opt-in, log
        /// level).
        asks: Vec<u64>,
        /// Ids of ordinary requests.
        requests: Vec<u64>,
    }

    impl BoardDouble {
        fn new(nonce: u32) -> Self {
            Self::on(LinkConfig::usb(), nonce)
        }

        fn on(config: LinkConfig, nonce: u32) -> Self {
            BoardDouble {
                link: Link::new(config, nonce),
                table: LearnedTable::default(),
                packed: false,
                pack_format: PACK_FORMAT_VERSION,
                asks: Vec::new(),
                requests: Vec::new(),
            }
        }

        fn reboot(&mut self, nonce: u32) {
            let (asks, pack_format) = (core::mem::take(&mut self.asks), self.pack_format);
            *self = BoardDouble::new(nonce);
            self.asks = asks;
            self.pack_format = pack_format;
        }

        fn serve(&mut self) {
            #[cfg(feature = "secure-link")]
            secure::answer_lookups(&mut self.link);
            while let Some(event) = self.link.recv() {
                match event {
                    LinkEvent::Up { .. } | LinkEvent::Reset { .. } => {
                        self.packed = false;
                        self.table = LearnedTable::default();
                        if matches!(event, LinkEvent::Up { .. }) {
                            self.send(&hello(self.pack_format));
                        }
                    }
                    LinkEvent::Message {
                        channel: CH_PROTO,
                        data,
                    } => {
                        let request = crate::decode_client_payload(&data).unwrap();
                        self.on_request(request);
                    }
                    _ => {}
                }
            }
        }

        fn on_request(&mut self, request: ClientMessage) {
            match request.msg {
                ClientRequest::SetEncoding { encoding, .. } => {
                    self.asks.push(request.id);
                    self.send(&WireServerMessage::new(
                        request.id,
                        ServerMsgBody::SetEncoding { encoding },
                    ));
                    self.packed = encoding == WireEncoding::Packed;
                }
                ClientRequest::SetLogLevel { .. } => {
                    self.asks.push(request.id);
                    self.send(&WireServerMessage::new(
                        request.id,
                        ServerMsgBody::SetLogLevel,
                    ));
                }
                _ => {
                    self.requests.push(request.id);
                    self.reply(request.id);
                }
            }
        }

        fn reply(&mut self, id: u64) {
            self.send(&WireServerMessage::new(
                id,
                ServerMsgBody::Error {
                    error: format!("a reply with some repeated words in it, for id {id}"),
                },
            ));
        }

        fn send(&mut self, message: &WireServerMessage) {
            #[cfg(feature = "ser-write-json")]
            let out = {
                let mut out = Vec::new();
                let table: Option<&mut dyn LearnStore> = if self.packed {
                    Some(&mut self.table)
                } else {
                    None
                };
                crate::encode_server_payload(message, table, &mut out);
                out
            };
            // Without the board's serializer the double cannot pack (the
            // packed tests need `ser-write-json`).
            #[cfg(not(feature = "ser-write-json"))]
            let out = crate::json::to_string(message).unwrap().into_bytes();
            self.link.send(CH_PROTO, &out).unwrap();
        }
    }

    fn hello(pack_format: u8) -> WireServerMessage {
        WireServerMessage::new(
            0,
            ServerMsgBody::Hello(ServerHello {
                proto: WIRE_PROTO_VERSION,
                build: BuildFacts {
                    features: vec![],
                    package: "fw-esp32c6".to_string(),
                    version: "unknown".into(),
                    commit: "unknown".to_string(),
                    dirty: false,
                    profile: "release-esp32".to_string(),
                },
                hardware: HardwareFacts::default(),
                device_uid: None,
                pack_format,
                auth: crate::HelloAuth::TRUSTED,
            }),
        )
    }

    /// The board double's key table, for secure ports.
    #[cfg(feature = "secure-link")]
    mod secure {
        use super::*;
        use lp_link::secure_channel::{KeyId, Psk, RefusalReason, SecureEvent};

        pub const KEY_ID: KeyId = KeyId([1; 16]);

        pub fn psk() -> Psk {
            Psk::new([2; 32])
        }

        /// Test entropy: a different fill every call, never a real RNG.
        pub fn entropy(buf: &mut [u8]) {
            static NEXT: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(1);
            for b in buf {
                *b = NEXT.fetch_add(29, core::sync::atomic::Ordering::Relaxed);
            }
        }

        /// The board's edge: [`KEY_ID`] is known, nothing else is.
        pub fn answer_lookups(link: &mut Link<SelectiveRepeat>) {
            while let Some(event) = link.poll_secure_event() {
                if let SecureEvent::KeyLookup { key_id } = event {
                    if key_id == KEY_ID {
                        link.provide_keys(key_id, &[psk()]);
                    } else {
                        link.refuse(key_id, RefusalReason::UnknownKey, 0);
                    }
                }
            }
        }
    }
}
