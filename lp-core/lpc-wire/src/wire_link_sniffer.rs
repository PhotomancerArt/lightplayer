//! Tools: a captured device link, read as wire messages and console lines.
//!
//! [`lp_link::sniffer::LinkSniffer`] turns captured bytes into link messages
//! without being either end; this adds the wire on top, the way a
//! [`WireLinkPort`](crate::WireLinkPort) reads it: proto-channel payloads
//! decoded ([`decode_server_payload`], with the capture's own learned table
//! for the board's packed replies, reset with every session), log records and
//! raw text as console lines ([`crate::console_line`]). For `lp-cli wire
//! unpack`, the emulator's wire tap and `lp-cli record timeline` (plan
//! `lp2025/2026-09-27-0215-lp-link-usb-cutover`, D10; BLE joined this in
//! `lp2025/2026-09-28-1445-ble-on-lp-link` — [`Self::ble`] +
//! [`Self::push_datagram`], one already-delimited frame per call, no COBS).
//!
//! A capture decodes packed replies from its session's start. One that starts
//! mid-session reads frames **unverified** (the checksum key is not known yet)
//! and cannot read packed replies until the next session: each is reported
//! [`SniffedWire::Unreadable`], never guessed.

use alloc::boxed::Box;
use alloc::string::{String, ToString};

use lp_json_pack::{LearnStore, LearnedTable};
use lp_link::sniffer::{Direction, LinkSniffer, SniffEvent};
use lp_link::{CH_LOG, CH_PROTO, Micros};

use crate::console_line::{TextLines, log_record_lines};
use crate::link_payload::{ServerPayload, decode_server_payload};

/// One thing read off a captured device link, in capture order.
#[derive(Debug)]
pub enum SniffedWire {
    /// `dir`'s sender started a new session (boot, reload, reset): the
    /// board's hello comes next, and packed replies start from an empty table.
    Session { dir: Direction, nonce: u32 },
    /// A board→host wire message. `verified`: its frames passed their
    /// checksums (false only before the capture's first handshake).
    Server {
        payload: ServerPayload,
        verified: bool,
    },
    /// A host→board request's JSON (hosts never pack requests).
    Client { json: String, verified: bool },
    /// A console line: a log record (`[LEVEL] text`) or raw text outside
    /// frames.
    Console { dir: Direction, line: String },
    /// A proto message that could not be read, and why (a packed reply from
    /// before the capture's first session, or a damaged capture).
    Unreadable {
        dir: Direction,
        len: usize,
        reason: String,
    },
    /// A frame that failed its checksum or its framing (the link resent it).
    Damaged { dir: Direction },
    /// Frames the capture never saw, even resent (a gap in the capture, not
    /// in the link).
    Gap { dir: Direction, skipped: u8 },
}

/// A captured device link, read as wire traffic. See the module docs.
pub struct WireLinkSniffer {
    link: LinkSniffer,
    /// The board's packed-reply table, as a host would have kept it.
    table: Box<LearnedTable>,
    text: [TextLines; 2],
}

impl Default for WireLinkSniffer {
    fn default() -> Self {
        Self::new()
    }
}

impl WireLinkSniffer {
    /// A sniffer for a USB-Serial-JTAG link ([`lp_link::LinkConfig::usb`]).
    pub fn new() -> Self {
        WireLinkSniffer {
            link: LinkSniffer::usb(),
            table: LearnedTable::boxed(),
            text: [TextLines::new(), TextLines::new()],
        }
    }

    /// A sniffer for a BLE link ([`lp_link::LinkConfig::ble`]): Datagram
    /// framing, fed with [`Self::push_datagram`] instead of [`Self::push`].
    pub fn ble() -> Self {
        WireLinkSniffer {
            link: LinkSniffer::ble(),
            table: LearnedTable::boxed(),
            text: [TextLines::new(), TextLines::new()],
        }
    }

    /// Feed bytes that went `dir` at `now` (any split; `now` only matters to
    /// the deframer's idle flush, so a tool without timestamps may pass 0).
    pub fn push(
        &mut self,
        dir: Direction,
        now: Micros,
        bytes: &[u8],
        mut on: impl FnMut(SniffedWire),
    ) {
        let Self { link, table, text } = self;
        link.push(dir, now, bytes, |event| {
            read_event(table, text, event, &mut on);
        });
    }

    /// Feed one already-delimited datagram frame — a GATT write or
    /// notification — the way a BLE link is tapped (see
    /// [`lp_link::sniffer::LinkSniffer::push_datagram`]).
    pub fn push_datagram(&mut self, dir: Direction, bytes: &[u8], mut on: impl FnMut(SniffedWire)) {
        let Self { link, table, text } = self;
        link.push_datagram(dir, bytes, |event| {
            read_event(table, text, event, &mut on);
        });
    }

    /// The capture ended: hand up text still waiting for a newline.
    pub fn flush(&mut self, mut on: impl FnMut(SniffedWire)) {
        for dir in [Direction::BoardToHost, Direction::HostToBoard] {
            let Self { link, table, text } = self;
            link.flush(dir, |event| read_event(table, text, event, &mut on));
            if let Some(line) = text[text_index(dir)].flush() {
                on(SniffedWire::Console { dir, line });
            }
        }
    }
}

fn text_index(dir: Direction) -> usize {
    match dir {
        Direction::BoardToHost => 0,
        Direction::HostToBoard => 1,
    }
}

fn read_event(
    table: &mut LearnedTable,
    text: &mut [TextLines; 2],
    event: SniffEvent,
    on: &mut impl FnMut(SniffedWire),
) {
    match event {
        SniffEvent::Session { dir, nonce } => {
            table.reset(0);
            on(SniffedWire::Session { dir, nonce });
        }
        SniffEvent::Text { dir, text: bytes } => {
            text[text_index(dir)].push(&bytes, |line| on(SniffedWire::Console { dir, line }));
        }
        SniffEvent::Damaged { dir } => on(SniffedWire::Damaged { dir }),
        SniffEvent::Gap { dir, skipped } => on(SniffedWire::Gap { dir, skipped }),
        SniffEvent::Message {
            dir,
            channel: CH_LOG,
            data,
            ..
        } => {
            for line in log_record_lines(&data) {
                on(SniffedWire::Console { dir, line });
            }
        }
        SniffEvent::Message {
            dir,
            channel: CH_PROTO,
            data,
            verified,
        } => {
            let read = match dir {
                Direction::BoardToHost => decode_server_payload(&data, table)
                    .map(|payload| SniffedWire::Server { payload, verified }),
                Direction::HostToBoard => core::str::from_utf8(&data)
                    .map(|json| SniffedWire::Client {
                        json: json.to_string(),
                        verified,
                    })
                    .map_err(|_| crate::PayloadError::NotUtf8),
            };
            on(read.unwrap_or_else(|error| SniffedWire::Unreadable {
                dir,
                len: data.len(),
                reason: error.to_string(),
            }));
        }
        // Other channels are unused on a device link.
        SniffEvent::Message { .. } => {}
    }
}

#[cfg(all(test, feature = "ser-write-json"))]
mod tests {
    use super::*;
    use crate::server::ServerMsgBody;
    use crate::{ClientMessage, ClientRequest, WireServerMessage, encode_client_payload};
    use alloc::vec;
    use alloc::vec::Vec;
    use lp_link::{Link, LinkConfig, SelectiveRepeat};

    #[test]
    fn a_whole_session_reads_as_requests_replies_and_console_lines() {
        let mut run = Pair::new();
        run.settle();
        run.host_sends(1);
        run.board_replies(1, false);
        run.board.send(CH_LOG, b"\x03main: tick").unwrap();
        // The host opts in; from here the board packs against a fresh table.
        run.board_replies_packed_from_now();
        for id in 2..20 {
            run.board_replies(id, true);
        }
        run.settle();
        run.capture
            .push((Direction::BoardToHost, b"[INIT] marker\r\n".to_vec()));

        let reads = run.sniff();
        let servers: Vec<(u64, bool)> = reads
            .iter()
            .filter_map(|r| match r {
                SniffedWire::Server { payload, verified } => {
                    assert!(*verified);
                    Some((payload.message.as_ref().unwrap().id, payload.packed))
                }
                _ => None,
            })
            .collect();
        assert_eq!(servers.len(), 19);
        assert_eq!(servers[0], (1, false));
        assert!(servers[1..].iter().all(|(_, packed)| *packed));
        let clients = reads
            .iter()
            .filter(|r| matches!(r, SniffedWire::Client { .. }))
            .count();
        assert_eq!(clients, 1);
        let console: Vec<&str> = reads
            .iter()
            .filter_map(|r| match r {
                SniffedWire::Console { line, .. } => Some(line.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(console, vec!["[INFO] main: tick", "[INIT] marker"]);
        assert!(
            !reads
                .iter()
                .any(|r| matches!(r, SniffedWire::Unreadable { .. }))
        );
    }

    #[test]
    fn a_capture_that_starts_after_the_table_learned_says_so() {
        let mut run = Pair::new();
        run.settle();
        run.board_replies_packed_from_now();
        run.board_replies(1, true);
        run.settle();
        run.capture.clear();
        run.board_replies(2, true);
        run.settle();
        let reads = run.sniff();
        assert!(
            reads
                .iter()
                .any(|r| matches!(r, SniffedWire::Unreadable { .. })),
            "{reads:?}"
        );
    }

    struct Pair {
        board: Link<SelectiveRepeat>,
        host: Link<SelectiveRepeat>,
        table: LearnedTable,
        now: Micros,
        capture: Vec<(Direction, Vec<u8>)>,
    }

    impl Pair {
        fn new() -> Self {
            Pair {
                board: Link::new(LinkConfig::usb(), 0x0B0A_0001),
                host: Link::new(LinkConfig::usb(), 0x0405_0001),
                table: LearnedTable::default(),
                now: 0,
                capture: Vec::new(),
            }
        }

        fn host_sends(&mut self, id: u64) {
            let request = ClientMessage {
                id,
                msg: ClientRequest::Hello,
            };
            self.host
                .send(CH_PROTO, &encode_client_payload(&request))
                .unwrap();
        }

        fn board_replies_packed_from_now(&mut self) {
            self.table = LearnedTable::default();
        }

        fn board_replies(&mut self, id: u64, packed: bool) {
            let message = WireServerMessage::new(
                id,
                ServerMsgBody::Error {
                    error: alloc::format!("a reply with repeated words, number {id}"),
                },
            );
            let mut out = Vec::new();
            let table: Option<&mut dyn LearnStore> = packed.then_some(&mut self.table);
            crate::encode_server_payload(&message, table, &mut out);
            self.board.send(CH_PROTO, &out).unwrap();
            self.settle();
        }

        fn settle(&mut self) {
            for _ in 0..100 {
                while let Some(frame) = self.board.poll_transmit(self.now) {
                    let frame = frame.to_vec();
                    self.host.on_bytes(self.now, &frame);
                    self.capture.push((Direction::BoardToHost, frame));
                }
                while let Some(frame) = self.host.poll_transmit(self.now) {
                    let frame = frame.to_vec();
                    self.board.on_bytes(self.now, &frame);
                    self.capture.push((Direction::HostToBoard, frame));
                }
                while self.board.recv().is_some() {}
                while self.host.recv().is_some() {}
                self.now += 1_000;
            }
        }

        fn sniff(&self) -> Vec<SniffedWire> {
            let mut sniffer = WireLinkSniffer::new();
            let mut reads = Vec::new();
            for (dir, bytes) in &self.capture {
                sniffer.push(*dir, 0, bytes, |r| reads.push(r));
            }
            sniffer.flush(|r| reads.push(r));
            reads
        }
    }
}
