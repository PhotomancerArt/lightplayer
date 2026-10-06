//! Test support: a real two-way lp-link capture, made in-process.
//!
//! A host [`WireLinkPort`] and a board double (a board-side [`Link`] that
//! says hello first on every session, answers the packed opt-in and then
//! sends its replies, packed when asked) run over a lossless pipe, and every
//! chunk each end wrote is kept in order: the bytes a serial capture, an emu
//! wire tap or a Studio recording would hold.

use lpc_wire::lp_link::sniffer::Direction;
use lpc_wire::lp_link::{CH_PROTO, Link, LinkConfig, LinkEvent, SelectiveRepeat};
use lpc_wire::{
    ClientRequest, LearnStore, LearnedTable, PACK_FORMAT_VERSION, ServerMsgBody,
    WIRE_PROTO_VERSION, WireEncoding, WireLinkPort, WireServerMessage,
};

/// What went each way, in order.
pub struct Capture {
    /// Every chunk, with the direction it went.
    pub chunks: Vec<(Direction, Vec<u8>)>,
}

impl Capture {
    /// The board → host bytes, concatenated (a raw serial capture).
    pub fn to_host(&self) -> Vec<u8> {
        self.bytes(Direction::BoardToHost)
    }

    /// The host → board bytes, concatenated.
    pub fn to_board(&self) -> Vec<u8> {
        self.bytes(Direction::HostToBoard)
    }

    fn bytes(&self, dir: Direction) -> Vec<u8> {
        self.chunks
            .iter()
            .filter(|(d, _)| *d == dir)
            .flat_map(|(_, bytes)| bytes.iter().copied())
            .collect()
    }
}

/// A session: `boot_text` raw first, the handshake, the hello, the opt-in
/// when `packed`, then `replies` (after one request the host sends, id 1).
pub fn capture(boot_text: &str, replies: &[WireServerMessage], packed: bool) -> Capture {
    capture_over(
        LinkConfig::usb(),
        LinkConfig::usb(),
        boot_text,
        replies,
        packed,
    )
}

/// [`capture`] with each end's link configuration named: a classic's UART
/// link is `LinkConfig::uart()` on the host and the board's own cut of it.
pub fn capture_over(
    host_config: LinkConfig,
    board_config: LinkConfig,
    boot_text: &str,
    replies: &[WireServerMessage],
    packed: bool,
) -> Capture {
    let mut host = WireLinkPort::new(host_config, 0xC0DE_0001, packed);
    let mut board = Board::new(board_config, packed);
    let mut chunks = vec![(Direction::BoardToHost, boot_text.as_bytes().to_vec())];
    host.on_bytes(0, boot_text.as_bytes());
    let mut asked = false;
    let mut sent_replies = false;
    for step in 0..400u64 {
        let now = step * 1_000;
        while let Some(frame) = host.poll_transmit(now) {
            let frame = frame.to_vec();
            board.link.on_bytes(now, &frame);
            chunks.push((Direction::HostToBoard, frame));
        }
        board.serve();
        if board.settled && !sent_replies {
            for reply in replies {
                board.send(reply);
            }
            sent_replies = true;
        }
        while let Some(frame) = board.link.poll_transmit(now) {
            let frame = frame.to_vec();
            host.on_bytes(now, &frame);
            chunks.push((Direction::BoardToHost, frame));
        }
        while host.poll_read().is_some() {}
        if board.settled && !asked {
            host.send_client(&lpc_wire::ClientMessage {
                id: 1,
                msg: ClientRequest::Hello,
            })
            .unwrap();
            asked = true;
        }
    }
    Capture { chunks }
}

/// The board's end, as firmware runs it.
struct Board {
    link: Link<SelectiveRepeat>,
    table: LearnedTable,
    packs: bool,
    packed: bool,
    /// The hello is out and the opt-in (if any) answered.
    settled: bool,
}

impl Board {
    fn new(config: LinkConfig, packs: bool) -> Self {
        Self {
            link: Link::new(config, 0xB0A2_0001),
            table: LearnedTable::default(),
            packs,
            packed: false,
            settled: false,
        }
    }

    fn serve(&mut self) {
        while let Some(event) = self.link.recv() {
            match event {
                LinkEvent::Up { .. } => {
                    self.table.reset(0);
                    self.send(&hello(if self.packs { PACK_FORMAT_VERSION } else { 0 }));
                    self.settled = !self.packs;
                }
                LinkEvent::Message {
                    channel: CH_PROTO,
                    data,
                } => {
                    let request = lpc_wire::decode_client_payload(&data).unwrap();
                    match request.msg {
                        ClientRequest::SetEncoding { encoding, .. } => {
                            self.send(&WireServerMessage::new(
                                request.id,
                                ServerMsgBody::SetEncoding { encoding },
                            ));
                            self.packed = encoding == WireEncoding::Packed;
                            self.settled = true;
                        }
                        _ => self.send(&WireServerMessage::new(
                            request.id,
                            ServerMsgBody::UnloadProject,
                        )),
                    }
                }
                _ => {}
            }
        }
    }

    fn send(&mut self, message: &WireServerMessage) {
        let mut payload = Vec::new();
        let table: Option<&mut dyn LearnStore> = if self.packed {
            Some(&mut self.table)
        } else {
            None
        };
        lpc_wire::encode_server_payload(message, table, &mut payload);
        self.link.send(CH_PROTO, &payload).unwrap();
    }
}

/// A board's hello offering `pack_format`.
pub fn hello(pack_format: u8) -> WireServerMessage {
    WireServerMessage::new(
        0,
        ServerMsgBody::Hello(lpc_wire::ServerHello {
            proto: WIRE_PROTO_VERSION,
            build: lpc_wire::BuildFacts {
                features: vec![],
                package: "fw-esp32c6".to_string(),
                version: "unknown".into(),
                commit: "unknown".to_string(),
                dirty: false,
                profile: "release-esp32".to_string(),
            },
            hardware: Default::default(),
            device_uid: None,
            pack_format,
            auth: lpc_wire::HelloAuth::TRUSTED,
            firmware: None,
        }),
    )
}

/// A reply with repeated words, so a packed session has something to learn.
pub fn log_reply(id: u64) -> WireServerMessage {
    WireServerMessage::new(
        id,
        ServerMsgBody::Log {
            level: lpc_wire::server::api::LogLevel::Info,
            message: format!("a reply with some repeated words in it, number {id}"),
        },
    )
}
