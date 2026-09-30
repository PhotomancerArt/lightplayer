//! A passive reader of a captured link: bytes in (one direction or both,
//! tagged), whole messages and console text out, without being either end.
//!
//! For tools that read what went over a wire after the fact or beside it
//! (`lp-cli wire unpack`, the emulator's wire tap, `lp-cli record timeline`;
//! plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`, D10). It knows frames,
//! not sessions: it deframes (COBS-FF, text outside frames), parses headers,
//! reassembles each direction's reliable messages in sequence order (resends
//! arrive late and twice; both are handled), and passes best-effort messages
//! (logs) straight through.
//!
//! **Checksums.** Frames are keyed with both ends' nonces, which only the
//! handshake says. A SYN names its sender's nonce and the one it believes
//! its peer has, so once the capture has seen a SYN from each side (or one
//! that names both) every later frame is verified, and a frame that fails is
//! reported [`SniffEvent::Damaged`] and dropped. A capture that starts
//! mid-session has seen no SYN: its frames are read **unverified** (the
//! checksum is skipped, and every message says so) until the next handshake.
//!
//! **Sessions.** A SYN with a nonce the capture has not seen for that side is
//! a new session ([`SniffEvent::Session`]): both directions' reassembly starts
//! over, and so must any per-session state the caller keeps (the learned wire
//! dictionary).

use alloc::vec::Vec;

use crate::Micros;
use crate::cobs;
use crate::crc::CrcKind;
use crate::deframer::{Deframed, Deframer, IdleFlush};
use crate::frame::{self, FrameKind, HEADER_LEN, Header, SynBody};

/// The largest frame payload the sniffer accepts (every preset is below it).
const MAX_PAYLOAD: usize = 2048;

/// Reliable frames held out of order per direction before the sniffer
/// decides the capture lost one for good.
const MAX_HELD: usize = 64;

/// Which way bytes went. The tool picks the ends' names; a device link's
/// convention is the board and the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    BoardToHost,
    HostToBoard,
}

impl Direction {
    fn index(self) -> usize {
        match self {
            Direction::BoardToHost => 0,
            Direction::HostToBoard => 1,
        }
    }

    fn other(self) -> Direction {
        match self {
            Direction::BoardToHost => Direction::HostToBoard,
            Direction::HostToBoard => Direction::BoardToHost,
        }
    }
}

/// What the sniffer read, in the order the bytes said it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SniffEvent {
    /// `dir`'s sender started a new session (a boot, a reload, a reset) with
    /// `nonce`. Per-session state kept above (a learned table) starts over.
    Session { dir: Direction, nonce: u32 },
    /// A whole message. `verified`: every frame of it passed its checksum
    /// (see the module docs); `false` only before the capture's first
    /// handshake.
    Message {
        dir: Direction,
        channel: u8,
        data: Vec<u8>,
        verified: bool,
    },
    /// Console text outside frames (boot text, a panic, a raw print), cut at
    /// newlines as the link cuts it.
    Text { dir: Direction, text: Vec<u8> },
    /// A frame that failed its checksum, its COBS or its header: damaged on
    /// the way (the sender resends it), or garbage.
    Damaged { dir: Direction },
    /// Reliable frames the capture never saw, even resent: the message they
    /// belonged to is lost to the capture (not to the link). `skipped` frames.
    Gap { dir: Direction, skipped: u8 },
}

/// One reliable frame waiting for its turn.
struct HeldFrame {
    seq: u8,
    first: bool,
    fin: bool,
    chan: u8,
    data: Vec<u8>,
    verified: bool,
}

/// One direction's reassembly.
#[derive(Default)]
struct Reassembly {
    /// The next reliable sequence number in order, once one has been seen.
    expected: Option<u8>,
    held: Vec<HeldFrame>,
    /// The message being put together: channel, bytes, all verified.
    partial: Option<(u8, Vec<u8>, bool)>,
}

impl Reassembly {
    /// A new session: sequence numbers start at 0 on both ends, so a first
    /// frame the capture missed (and sees only resent, later) is still read
    /// in its place. A capture that starts mid-session has no such anchor and
    /// starts from the first frame it sees.
    fn restart(&mut self) {
        *self = Reassembly {
            expected: Some(0),
            ..Reassembly::default()
        };
    }
}

/// One captured direction's byte stream.
struct DirStream {
    deframer: Deframer,
    raw: Vec<u8>,
    reassembly: Reassembly,
}

/// The passive reader. See the module docs.
pub struct LinkSniffer {
    crc: CrcKind,
    escape_ff: bool,
    streams: [DirStream; 2],
    /// Each side's nonce as the capture last heard it (index = the side's
    /// sending direction).
    nonces: [Option<u32>; 2],
    /// The previous session's key, so a frame in flight across a reset is
    /// dropped quietly instead of reported damaged.
    prev_key: Option<u32>,
}

impl LinkSniffer {
    /// A sniffer for stream framing with `crc`; `escape_ff` as the link's
    /// [`LinkConfig::escape_ff`](crate::LinkConfig::escape_ff).
    pub fn new(crc: CrcKind, escape_ff: bool) -> Self {
        let max_frame = cobs::max_encoded_no_ff_len(HEADER_LEN + MAX_PAYLOAD + crc.len());
        let stream = || DirStream {
            deframer: Deframer::new(max_frame, false).with_text_mark(escape_ff),
            raw: Vec::new(),
            reassembly: Reassembly::default(),
        };
        LinkSniffer {
            crc,
            escape_ff,
            streams: [stream(), stream()],
            nonces: [None, None],
            prev_key: None,
        }
    }

    /// A sniffer for [`LinkConfig::usb`](crate::LinkConfig::usb)'s framing.
    pub fn usb() -> Self {
        Self::new(CrcKind::Crc32c, true)
    }

    /// A sniffer for [`LinkConfig::ble`](crate::LinkConfig::ble)'s Datagram
    /// framing. `escape_ff` is meaningless here (no byte-stream deframer is
    /// ever used — see [`push_datagram`](Self::push_datagram)); passed as
    /// `false` for a config that would otherwise be inert.
    pub fn ble() -> Self {
        Self::new(CrcKind::Crc32c, false)
    }

    /// Whether frames are being verified (a handshake has named both nonces).
    pub fn is_verifying(&self) -> bool {
        self.key().is_some()
    }

    /// Feed bytes that went `dir` at `now` (any split), calling `on` for
    /// everything they complete.
    pub fn push(
        &mut self,
        dir: Direction,
        now: Micros,
        bytes: &[u8],
        mut on: impl FnMut(SniffEvent),
    ) {
        for &b in bytes {
            let stream = &mut self.streams[dir.index()];
            match stream.deframer.push(now, b) {
                Deframed::Nothing | Deframed::Abandoned | Deframed::Overflow => {}
                Deframed::Text => on(SniffEvent::Text {
                    dir,
                    text: stream.deframer.take_text(),
                }),
                Deframed::Frame => {
                    let mut raw = core::mem::take(&mut stream.raw);
                    raw.clear();
                    let body = stream.deframer.frame();
                    let decoded = if self.escape_ff {
                        frame::unwrap_stream(body, &mut raw)
                    } else {
                        frame::unwrap_stream_plain(body, &mut raw)
                    };
                    let ok = decoded.is_ok() && self.on_frame(dir, &raw, &mut on);
                    if decoded.is_err() {
                        on(SniffEvent::Damaged { dir });
                    }
                    let stream = &mut self.streams[dir.index()];
                    stream.deframer.frame_done(ok);
                    stream.raw = raw;
                }
            }
        }
    }

    /// Feed one already-delimited datagram frame — a GATT write or
    /// notification, a UDP/WebSocket message
    /// ([`Framing::Datagram`](crate::Framing::Datagram)) — calling `on` for
    /// what it completes. No COBS, no reassembly across calls: `bytes` is
    /// exactly one frame, the way
    /// [`Link::on_datagram`](crate::Link::on_datagram) reads it. A frame
    /// that fails to parse or verify is reported [`SniffEvent::Damaged`] by
    /// [`Self::on_frame`] itself; nothing else to report here.
    pub fn push_datagram(&mut self, dir: Direction, bytes: &[u8], mut on: impl FnMut(SniffEvent)) {
        self.on_frame(dir, bytes, &mut on);
    }

    /// The capture ended (or went quiet): hand up text still waiting for a
    /// newline in `dir`.
    pub fn flush(&mut self, dir: Direction, mut on: impl FnMut(SniffEvent)) {
        let deframer = &mut self.streams[dir.index()].deframer;
        match deframer.flush_idle() {
            IdleFlush::Text => on(SniffEvent::Text {
                dir,
                text: deframer.take_text(),
            }),
            IdleFlush::Garbage => on(SniffEvent::Damaged { dir }),
            IdleFlush::Nothing => {}
        }
    }

    fn key(&self) -> Option<u32> {
        Some(self.nonces[0]? ^ self.nonces[1]?)
    }

    /// One decoded frame; `true` if it was readable (the deframer's resync
    /// rule).
    fn on_frame(&mut self, dir: Direction, raw: &[u8], on: &mut impl FnMut(SniffEvent)) -> bool {
        let Some(hdr) = Header::parse(raw) else {
            on(SniffEvent::Damaged { dir });
            return false;
        };
        if hdr.kind == FrameKind::Syn {
            let Some(syn) = frame::verify(self.crc, 0, raw).and_then(SynBody::parse) else {
                on(SniffEvent::Damaged { dir });
                return false;
            };
            self.on_syn(dir, syn, on);
            return true;
        }
        let (body, verified) = match self.key() {
            Some(key) => match frame::verify(self.crc, key, raw) {
                Some(body) => (body, true),
                None => {
                    if self
                        .prev_key
                        .is_some_and(|k| frame::verify(self.crc, k, raw).is_some())
                    {
                        // From the previous session, in flight across a reset.
                        return true;
                    }
                    on(SniffEvent::Damaged { dir });
                    return false;
                }
            },
            None => {
                let n = self.crc.len();
                if raw.len() < HEADER_LEN + n {
                    on(SniffEvent::Damaged { dir });
                    return false;
                }
                (&raw[HEADER_LEN..raw.len() - n], false)
            }
        };
        match hdr.kind {
            FrameKind::Data => self.on_data(dir, &hdr, body, verified, on),
            FrameKind::Datagram => on(SniffEvent::Message {
                dir,
                channel: hdr.chan,
                data: body.to_vec(),
                verified,
            }),
            FrameKind::Ack | FrameKind::Syn => {}
        }
        true
    }

    fn on_syn(&mut self, dir: Direction, syn: SynBody, on: &mut impl FnMut(SniffEvent)) {
        let side = dir.index();
        if self.nonces[side] != Some(syn.nonce) {
            if let Some(key) = self.key() {
                self.prev_key = Some(key);
            }
            self.nonces[side] = Some(syn.nonce);
            // Both ends' sequence spaces restart with a session.
            for stream in &mut self.streams {
                stream.reassembly.restart();
            }
            on(SniffEvent::Session {
                dir,
                nonce: syn.nonce,
            });
        }
        let peer = dir.other().index();
        if self.nonces[peer].is_none() && syn.your != 0 {
            self.nonces[peer] = Some(syn.your);
        }
    }

    fn on_data(
        &mut self,
        dir: Direction,
        hdr: &Header,
        body: &[u8],
        verified: bool,
        on: &mut impl FnMut(SniffEvent),
    ) {
        let r = &mut self.streams[dir.index()].reassembly;
        let expected = *r.expected.get_or_insert(hdr.seq);
        let ahead = hdr.seq.wrapping_sub(expected);
        if ahead >= 128 || r.held.iter().any(|h| h.seq == hdr.seq) {
            return; // A resend of something already read.
        }
        r.held.push(HeldFrame {
            seq: hdr.seq,
            first: hdr.first,
            fin: hdr.fin,
            chan: hdr.chan,
            data: body.to_vec(),
            verified,
        });
        drain_in_order(dir, r, on);
        if r.held.len() > MAX_HELD {
            // The capture lost a frame the link resent where we could not
            // see it (or never resent): skip to what we have.
            let expected = r.expected.unwrap_or(0);
            let nearest = r
                .held
                .iter()
                .map(|h| h.seq.wrapping_sub(expected))
                .min()
                .unwrap_or(0);
            r.expected = Some(expected.wrapping_add(nearest));
            r.partial = None;
            on(SniffEvent::Gap {
                dir,
                skipped: nearest,
            });
            drain_in_order(dir, r, on);
        }
    }
}

/// Deliver every held frame that is next in sequence.
fn drain_in_order(dir: Direction, r: &mut Reassembly, on: &mut impl FnMut(SniffEvent)) {
    while let Some(expected) = r.expected
        && let Some(at) = r.held.iter().position(|h| h.seq == expected)
    {
        let frame = r.held.swap_remove(at);
        r.expected = Some(expected.wrapping_add(1));
        if frame.first {
            r.partial = Some((frame.chan, Vec::new(), true));
        }
        // A middle fragment of a message whose start the capture missed.
        let Some((chan, data, all_verified)) = r.partial.as_mut() else {
            continue;
        };
        if *chan != frame.chan {
            r.partial = None;
            continue;
        }
        data.extend_from_slice(&frame.data);
        *all_verified &= frame.verified;
        if frame.fin
            && let Some((channel, data, verified)) = r.partial.take()
        {
            on(SniffEvent::Message {
                dir,
                channel,
                data,
                verified,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CH_LOG, CH_PROTO, Link, LinkConfig, LinkEvent, SelectiveRepeat};
    use alloc::vec;

    #[test]
    fn a_whole_capture_reads_every_message_verified() {
        let mut run = Pair::new();
        run.settle();
        let big: Vec<u8> = (0..1500u32).map(|i| (i % 251) as u8).collect();
        run.board.send(CH_PROTO, b"{\"hello\":1}").unwrap();
        run.host.send(CH_PROTO, &big).unwrap();
        run.board.send(CH_LOG, b"\x03main: tick").unwrap();
        run.settle();

        let events = sniff(&run.capture);
        let sessions = events
            .iter()
            .filter(|e| matches!(e, SniffEvent::Session { .. }))
            .count();
        assert_eq!(sessions, 2, "one per side");
        // Each direction in its own order (the two interleave as they will).
        let messages = messages_of(&events);
        let from = |dir| {
            messages
                .iter()
                .filter(|m| m.0 == dir)
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(
            from(Direction::BoardToHost),
            vec![
                (
                    Direction::BoardToHost,
                    CH_PROTO,
                    b"{\"hello\":1}".to_vec(),
                    true
                ),
                (
                    Direction::BoardToHost,
                    CH_LOG,
                    b"\x03main: tick".to_vec(),
                    true
                ),
            ]
        );
        assert_eq!(
            from(Direction::HostToBoard),
            vec![(Direction::HostToBoard, CH_PROTO, big, true)]
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, SniffEvent::Damaged { .. }))
        );
    }

    #[test]
    fn a_capture_that_starts_mid_session_reads_unverified() {
        let mut run = Pair::new();
        run.settle();
        run.capture.clear();
        run.board.send(CH_PROTO, b"after the handshake").unwrap();
        run.settle();
        let messages = messages_of(&sniff(&run.capture));
        assert_eq!(
            messages,
            vec![(
                Direction::BoardToHost,
                CH_PROTO,
                b"after the handshake".to_vec(),
                false
            )]
        );
    }

    #[test]
    fn text_between_frames_is_text_and_damage_is_reported() {
        let mut run = Pair::new();
        run.settle();
        run.capture
            .push((Direction::BoardToHost, b"[INIT] booting\n".to_vec()));
        run.board.send(CH_PROTO, b"one").unwrap();
        run.settle();
        let mut events = sniff(&run.capture);
        assert!(events.contains(&SniffEvent::Text {
            dir: Direction::BoardToHost,
            text: b"[INIT] booting\n".to_vec(),
        }));
        // A frame with a flipped byte, then its intact twin.
        let (_, frame) = run
            .capture
            .iter()
            .rev()
            .find(|(d, f)| *d == Direction::BoardToHost && f.len() > 12)
            .cloned()
            .unwrap();
        let mut bad = frame.clone();
        bad[5] ^= 0x10;
        let mut sniffer = LinkSniffer::usb();
        events.clear();
        for (dir, bytes) in &run.capture {
            sniffer.push(*dir, 0, bytes, |e| events.push(e));
        }
        sniffer.push(Direction::BoardToHost, 0, &bad, |e| events.push(e));
        assert!(matches!(events.last(), Some(SniffEvent::Damaged { .. })));
    }

    #[test]
    fn a_resent_frame_is_read_once_and_in_order() {
        let mut run = Pair::new();
        run.settle();
        run.drop_next_board_frame = true;
        run.board.send(CH_PROTO, &[b'a'; 600]).unwrap();
        run.board.send(CH_PROTO, b"second").unwrap();
        run.settle();
        let messages = messages_of(&sniff(&run.capture));
        assert_eq!(
            messages,
            vec![
                (Direction::BoardToHost, CH_PROTO, vec![b'a'; 600], true),
                (Direction::BoardToHost, CH_PROTO, b"second".to_vec(), true),
            ]
        );
        assert!(run.board.counters().retransmits > 0, "the drop was resent");
    }

    #[test]
    fn a_restart_is_a_new_session() {
        let mut run = Pair::new();
        run.settle();
        run.board.send(CH_PROTO, b"old").unwrap();
        run.settle();
        run.board.restart(run.now);
        run.settle();
        run.board.send(CH_PROTO, b"new").unwrap();
        run.settle();
        let events = sniff(&run.capture);
        let sessions = events
            .iter()
            .filter(|e| matches!(e, SniffEvent::Session { .. }))
            .count();
        assert!(sessions >= 3, "{sessions}");
        let messages = messages_of(&events);
        assert_eq!(messages.len(), 2);
        assert!(messages.iter().all(|m| m.3), "verified in both sessions");
    }

    /// BLE's shape (`lp2025/2026-09-28-1445-ble-on-lp-link`): each captured
    /// chunk is exactly one already-delimited datagram frame (a GATT write
    /// or notification), fed one call per frame — no COBS, no byte-stream
    /// reassembly. `push_datagram` reads a whole session's messages the same
    /// way `push` does for a byte stream, and reports a single mangled frame
    /// as damaged without losing the messages around it.
    #[test]
    fn a_datagram_capture_reads_one_frame_per_call() {
        let mut run = DatagramPair::new();
        run.settle();
        run.board.send(CH_PROTO, b"{\"hello\":1}").unwrap();
        run.host.send(CH_PROTO, b"a request").unwrap();
        run.settle();

        let mut sniffer = LinkSniffer::ble();
        let mut events = Vec::new();
        for (dir, frame) in &run.capture {
            sniffer.push_datagram(*dir, frame, |e| events.push(e));
        }
        let messages = messages_of(&events);
        assert!(
            messages.contains(&(
                Direction::BoardToHost,
                CH_PROTO,
                b"{\"hello\":1}".to_vec(),
                true
            )),
            "{messages:?}"
        );
        assert!(
            messages.contains(&(
                Direction::HostToBoard,
                CH_PROTO,
                b"a request".to_vec(),
                true
            )),
            "{messages:?}"
        );

        // One mangled frame, fed to the SAME (already-keyed) sniffer: damaged,
        // and nothing else about it.
        let mangled = {
            let mut frame = run
                .capture
                .iter()
                .find(|(dir, f)| *dir == Direction::BoardToHost && f.len() > HEADER_LEN)
                .map(|(_, f)| f.clone())
                .expect("at least one data frame");
            frame[5] ^= 0x40;
            frame
        };
        let mut damage_events = Vec::new();
        sniffer.push_datagram(Direction::BoardToHost, &mangled, |e| damage_events.push(e));
        assert_eq!(
            damage_events,
            vec![SniffEvent::Damaged {
                dir: Direction::BoardToHost
            }]
        );
    }

    type Msg = (Direction, u8, Vec<u8>, bool);

    fn messages_of(events: &[SniffEvent]) -> Vec<Msg> {
        events
            .iter()
            .filter_map(|e| match e {
                SniffEvent::Message {
                    dir,
                    channel,
                    data,
                    verified,
                } => Some((*dir, *channel, data.clone(), *verified)),
                _ => None,
            })
            .collect()
    }

    fn sniff(capture: &[(Direction, Vec<u8>)]) -> Vec<SniffEvent> {
        let mut sniffer = LinkSniffer::usb();
        let mut events = Vec::new();
        for (dir, bytes) in capture {
            sniffer.push(*dir, 0, bytes, |e| events.push(e));
        }
        events
    }

    /// Two links wired back to back, every byte that crossed recorded as
    /// the receiving end saw it.
    struct Pair {
        board: Link<SelectiveRepeat>,
        host: Link<SelectiveRepeat>,
        now: Micros,
        capture: Vec<(Direction, Vec<u8>)>,
        drop_next_board_frame: bool,
    }

    impl Pair {
        fn new() -> Self {
            Pair {
                board: Link::new(LinkConfig::usb(), 0x1111_2222),
                host: Link::new(LinkConfig::usb(), 0x3333_4444),
                now: 0,
                capture: Vec::new(),
                drop_next_board_frame: false,
            }
        }

        /// Run the pair for a simulated half second.
        fn settle(&mut self) {
            for _ in 0..500 {
                while let Some(frame) = self.board.poll_transmit(self.now) {
                    let frame = frame.to_vec();
                    let is_data = frame.len() > 16;
                    if self.drop_next_board_frame && is_data {
                        self.drop_next_board_frame = false;
                        continue;
                    }
                    self.host.on_bytes(self.now, &frame);
                    self.capture.push((Direction::BoardToHost, frame));
                }
                while let Some(frame) = self.host.poll_transmit(self.now) {
                    let frame = frame.to_vec();
                    self.board.on_bytes(self.now, &frame);
                    self.capture.push((Direction::HostToBoard, frame));
                }
                while let Some(ev) = self.board.recv() {
                    drop::<LinkEvent>(ev);
                }
                while self.host.recv().is_some() {}
                self.now += 1_000;
            }
        }
    }

    /// Two [`LinkConfig::ble`]-shaped links wired back to back, captured as
    /// discrete datagrams (`on_datagram`/`poll_transmit`) rather than a byte
    /// stream — every element is already exactly one frame, matching how a
    /// GATT write or notification is tapped (see [`push_datagram`]).
    struct DatagramPair {
        board: Link<SelectiveRepeat>,
        host: Link<SelectiveRepeat>,
        now: Micros,
        capture: Vec<(Direction, Vec<u8>)>,
    }

    impl DatagramPair {
        fn new() -> Self {
            DatagramPair {
                board: Link::new(LinkConfig::ble(), 0xB0A2_0001),
                host: Link::new(LinkConfig::ble(), 0x4051_0002),
                now: 0,
                capture: Vec::new(),
            }
        }

        fn settle(&mut self) {
            for _ in 0..500 {
                while let Some(frame) = self.board.poll_transmit(self.now) {
                    let frame = frame.to_vec();
                    self.host.on_datagram(self.now, &frame);
                    self.capture.push((Direction::BoardToHost, frame));
                }
                while let Some(frame) = self.host.poll_transmit(self.now) {
                    let frame = frame.to_vec();
                    self.board.on_datagram(self.now, &frame);
                    self.capture.push((Direction::HostToBoard, frame));
                }
                while let Some(ev) = self.board.recv() {
                    drop::<LinkEvent>(ev);
                }
                while self.host.recv().is_some() {}
                self.now += 1_000;
            }
        }
    }
}
