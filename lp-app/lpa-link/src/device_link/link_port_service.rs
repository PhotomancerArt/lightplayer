//! One browser port's lp-link end, and the queues its drainers read.
//!
//! A Web Serial port and a tab-hosted board each keep ONE of these for as
//! long as the port is open (plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`,
//! D1/D2). It is the sans-IO half of the provider's per-port loop: the edge
//! (`providers/browser_serial_esp32/browser_serial.rs`,
//! `providers/emulator_tab/emulator_tab_link_port.rs`) pulls bytes from the
//! page, hands them to [`on_bytes`](LinkPortService::on_bytes), writes what
//! [`transmit`](LinkPortService::transmit) hands back, and wakes it again
//! within [`wake_in`](LinkPortService::wake_in). Everything else — the
//! handshake, resends, acknowledgements, the packed-reply opt-in, the dev
//! log-level rider — is the [`WireLinkPort`]'s.
//!
//! What the board said lands in two queues:
//!
//! - **reads** ([`take_reads`](LinkPortService::take_reads)): messages,
//!   console lines and link resets, for whoever drains the port — the
//!   model's link pump or, while it holds the wire, a borrowed conversation.
//!   One queue, many drainers, one at a time (the exclusive borrow, D2); a
//!   borrower that takes a message takes it whole, because the link already
//!   delivered it whole.
//! - **notes** ([`take_notes`](LinkPortService::take_notes)): the link's own
//!   story for the device journal (up, stalled, the opt-in's outcome), which
//!   only the pump reads.
//!
//! A write that cannot be queued (the link's send budget is full, or the
//! message is larger than a link message may be) is an error the sender sees,
//! never a silent drop.

use std::collections::VecDeque;

use lpc_wire::lp_link::{LinkState, Micros};
use lpc_wire::server::api::LogLevel;
use lpc_wire::{ClientMessage, LinkCounters, WireLinkPort};

use crate::device_link::port_read_map::{MappedRead, map_port_read};
use crate::device_link::wire_reader::WireRead;

/// A browser port's lp-link end. See the module docs.
pub struct LinkPortService {
    port: WireLinkPort,
    reads: VecDeque<WireRead>,
    notes: Vec<String>,
    /// Whether the last look found the link stalled, so each edge is noted
    /// once.
    stalled: bool,
}

impl LinkPortService {
    /// A port with a fresh `nonce` (random per open: it is how the board
    /// tells this open from the last one). `want_packed` asks the board to
    /// pack its replies; `device_log` is the dev log-level rider.
    pub fn new(nonce: u32, want_packed: bool, device_log: Option<LogLevel>) -> Self {
        Self {
            port: WireLinkPort::new(nonce, want_packed).with_device_log_level(device_log),
            reads: VecDeque::new(),
            notes: Vec::new(),
            stalled: false,
        }
    }

    /// Bytes the page read from the port at `now`, in any split.
    pub fn on_bytes(&mut self, now: Micros, bytes: &[u8]) {
        if !bytes.is_empty() {
            self.port.on_bytes(now, bytes);
        }
        self.collect(now);
    }

    /// Hand every frame the link has to send now to `write`, in order.
    pub fn transmit(&mut self, now: Micros, mut write: impl FnMut(&[u8])) {
        while let Some(frame) = self.port.poll_transmit(now) {
            write(frame);
        }
        self.collect(now);
    }

    /// Queue a request already serialized as JSON (no `M!`, no newline).
    /// The caller transmits after.
    pub fn send_client_json(&mut self, json: &str) -> Result<(), String> {
        self.port
            .send_client_json(json)
            .map_err(|error| format!("the link would not take the request: {error:?}"))
    }

    /// [`Self::send_client_json`] for a request not yet serialized.
    pub fn send_client(&mut self, message: &ClientMessage) -> Result<(), String> {
        self.port
            .send_client(message)
            .map_err(|error| format!("the link would not take the request: {error:?}"))
    }

    /// How long until the link next needs [`Self::transmit`] for a timer,
    /// at most `cap` (new bytes and new sends need one too, and the edge
    /// polls the page for bytes on the same tick).
    pub fn wake_in(&self, now: Micros, cap: Micros) -> Micros {
        match self.port.poll_timeout() {
            Some(at) => at.saturating_sub(now).min(cap),
            None => cap,
        }
    }

    /// Everything the board said since the last take, in order.
    pub fn take_reads(&mut self) -> Vec<WireRead> {
        self.reads.drain(..).collect()
    }

    /// What the link said about itself since the last take.
    pub fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    /// Up (the handshake is done) or not.
    pub fn is_up(&self) -> bool {
        self.port.state() == LinkState::Established
    }

    /// Up, but the board has been silent past the link's stall time.
    pub fn is_stalled(&self, now: Micros) -> bool {
        self.port.is_stalled(now)
    }

    /// This end's link counters.
    pub fn counters(&self) -> LinkCounters {
        self.port.counters()
    }

    /// Sort what the port has read onto the two queues, and note a stall's
    /// edges.
    fn collect(&mut self, now: Micros) {
        while let Some(read) = self.port.poll_read() {
            match map_port_read(read) {
                MappedRead::Read(read) => self.reads.push_back(read),
                MappedRead::Note(note) => self.notes.push(note),
            }
        }
        let stalled = self.port.is_stalled(now);
        if stalled != self.stalled {
            self.stalled = stalled;
            self.notes.push(
                if stalled {
                    "link: stalled — the board has gone quiet; holding the session"
                } else {
                    "link: the board is answering again"
                }
                .to_string(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_wire::lp_link::{CH_PROTO, Link, LinkConfig, LinkEvent, SelectiveRepeat};
    use lpc_wire::server::hello::{BuildFacts, HardwareFacts, ServerHello};
    use lpc_wire::{
        ClientRequest, PACK_FORMAT_VERSION, ServerMsgBody, WIRE_PROTO_VERSION, WireServerMessage,
    };

    #[test]
    fn the_link_comes_up_and_the_hello_is_a_frame() {
        let mut bench = Bench::new();
        bench.run(40);

        let notes = bench.host.take_notes();
        assert!(
            notes.iter().any(|note| note.starts_with("link: up")),
            "{notes:?}"
        );
        let reads = bench.host.take_reads();
        assert!(
            matches!(
                reads.as_slice(),
                [WireRead::Frame(frame)] if frame.json.contains("\"hello\"") && !frame.packed
            ),
            "{reads:?}"
        );
        assert!(bench.host.is_up());
    }

    #[test]
    fn a_request_is_answered_through_the_link() {
        let mut bench = Bench::new();
        bench.run(40);
        bench.host.take_reads();

        let json = lpc_wire::json::to_string(&ClientMessage {
            id: 7,
            msg: ClientRequest::StopAllProjects,
        })
        .unwrap();
        bench.host.send_client_json(&json).expect("queued");
        bench.run(40);

        assert_eq!(bench.board.requests, [7]);
        let reads = bench.host.take_reads();
        assert!(
            matches!(
                reads.as_slice(),
                [WireRead::Frame(frame)] if frame.message.as_ref().is_ok_and(|m| m.id == 7)
            ),
            "{reads:?}"
        );
    }

    #[test]
    fn raw_text_outside_frames_is_console_lines() {
        let mut bench = Bench::new();
        bench.run(40);
        bench.host.take_reads();

        bench.host.on_bytes(bench.now, b"[INIT] boot marker\r\n");
        let reads = bench.host.take_reads();
        assert!(
            matches!(reads.as_slice(), [WireRead::Line(line)] if line == "[INIT] boot marker"),
            "{reads:?}"
        );
    }

    /// Plan D9: a board that restarts ends the session, and the drainer
    /// hears it at once — before the new session's hello.
    #[test]
    fn a_board_restart_is_a_link_reset_then_a_new_hello() {
        let mut bench = Bench::new();
        bench.run(40);
        bench.host.take_reads();
        bench.host.take_notes();

        bench.board = BoardDouble::new(0xB0A2_0002);
        bench.run(60);

        let reads = bench.host.take_reads();
        let reset_at = reads
            .iter()
            .position(
                |read| matches!(read, WireRead::LinkReset(note) if note.contains("restarted")),
            )
            .unwrap_or_else(|| panic!("no reset read: {reads:?}"));
        let hello_at = reads
            .iter()
            .position(
                |read| matches!(read, WireRead::Frame(frame) if frame.json.contains("\"hello\"")),
            )
            .unwrap_or_else(|| panic!("no new hello: {reads:?}"));
        assert!(reset_at < hello_at, "{reads:?}");
    }

    #[test]
    fn a_silent_board_is_noted_as_a_stall_and_its_return_as_news() {
        let mut bench = Bench::new();
        bench.run(40);
        bench.host.take_notes();

        // Two seconds of the host alone: the board says nothing.
        for _ in 0..2_000 {
            bench.host.transmit(bench.now, |_| {});
            bench.now += 1_000;
        }
        let notes = bench.host.take_notes();
        assert!(
            notes.iter().any(|note| note.contains("stalled")),
            "{notes:?}"
        );
        bench.run(40);
        let notes = bench.host.take_notes();
        assert!(
            notes.iter().any(|note| note.contains("answering again")),
            "{notes:?}"
        );
    }

    #[test]
    fn the_wake_is_capped() {
        let bench = Bench::new();
        assert!(bench.host.wake_in(0, 10_000) <= 10_000);
    }

    /// Both ends, a millisecond at a time.
    struct Bench {
        host: LinkPortService,
        board: BoardDouble,
        now: Micros,
    }

    impl Bench {
        fn new() -> Self {
            Self {
                host: LinkPortService::new(0xAAAA_0001, false, None),
                board: BoardDouble::new(0xB0A2_0001),
                now: 0,
            }
        }

        fn run(&mut self, steps: u32) {
            for _ in 0..steps {
                let board = &mut self.board.link;
                let now = self.now;
                self.host.transmit(now, |frame| board.on_bytes(now, frame));
                self.board.serve();
                let mut out = Vec::new();
                while let Some(frame) = self.board.link.poll_transmit(now) {
                    out.extend_from_slice(frame);
                }
                self.host.on_bytes(now, &out);
                self.now += 1_000;
            }
        }
    }

    /// A board's end: hello on every `Up`, and a reply to every request.
    struct BoardDouble {
        link: Link<SelectiveRepeat>,
        requests: Vec<u64>,
    }

    impl BoardDouble {
        fn new(nonce: u32) -> Self {
            Self {
                link: Link::new(LinkConfig::usb(), nonce),
                requests: Vec::new(),
            }
        }

        fn serve(&mut self) {
            while let Some(event) = self.link.recv() {
                match event {
                    LinkEvent::Up { .. } => self.send(&hello()),
                    LinkEvent::Message {
                        channel: CH_PROTO,
                        data,
                    } => {
                        let request = lpc_wire::decode_client_payload(&data).expect("a request");
                        self.requests.push(request.id);
                        self.send(&WireServerMessage::new(
                            request.id,
                            ServerMsgBody::StopAllProjects,
                        ));
                    }
                    _ => {}
                }
            }
        }

        fn send(&mut self, message: &WireServerMessage) {
            let mut payload = Vec::new();
            lpc_wire::encode_server_payload(message, None, &mut payload);
            self.link.send(CH_PROTO, &payload).expect("board send");
        }
    }

    fn hello() -> WireServerMessage {
        WireServerMessage::new(
            0,
            ServerMsgBody::Hello(ServerHello {
                proto: WIRE_PROTO_VERSION,
                build: BuildFacts {
                    features: vec![],
                    package: "fw-esp32c6".to_string(),
                    commit: "unknown".to_string(),
                    dirty: false,
                    profile: "release-esp32".to_string(),
                },
                hardware: HardwareFacts::default(),
                device_uid: None,
                pack_format: PACK_FORMAT_VERSION,
                auth: lpc_wire::HelloAuth::TRUSTED,
            }),
        )
    }
}
