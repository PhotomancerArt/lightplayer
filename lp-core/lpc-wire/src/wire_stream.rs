//! Hosts: a board's byte stream → console lines and wire messages, in either
//! encoding.
//!
//! A board writes three things onto one byte stream: console text, `M!{json}`
//! lines, and — on a link that opted in (plan `lp-json-pack`, Q1) — learned
//! packed frames, `\n 0x00 'L' COBS(header + packed) 0x00`
//! ([`ser_learned_frame_to`](crate::ser_learned_frame_to)). A packed frame may
//! hold any byte, `\n` included, so a plain line splitter tears it; this one
//! finds frames first ([`lp_json_pack::FrameScanner`]) and splits only the
//! text between them into lines.
//!
//! Every host reader goes through [`WireStream`], and every reader must accept
//! **both** forms at all times: a board resets its link to JSON when the cable
//! is pulled, when it reboots, and when the host stops draining for a while,
//! so a link agreed packed can fall back to JSON mid-session. A packed frame
//! decodes to exactly the JSON text its `M!` line would have carried, so the
//! caller handles [`WireChunk::Frame`] the same way whichever form it came in.
//!
//! # One stream per link, for the link's whole life
//!
//! Packed frames are coded against a table the board and this reader each
//! learn as frames go by (`lp_json_pack::pack_learned`; plan
//! `lp2025/2026-09-25-0006-learned-wire-dictionary`). So a [`WireStream`] is
//! stateful: keep **one** per link from the link's first byte, and
//! [`clear`](WireStream::clear) it when the link resets. Every frame's header
//! names the table state it was coded against. When it disagrees with this
//! reader's table — a frame that changed the board's table was lost or torn
//! in flight, or this reader started mid-connection — the frame is dropped as
//! [`WireChunk::Desync`], never decoded against the wrong names, and so is
//! every packed frame after it until the board resets (a frame coded against
//! the empty table: after an opt-in, or a reboot). The reader's owner answers a desync with the opt-in
//! request ([`PackOptIn::desynced`](crate::PackOptIn::desynced)), which starts
//! that reset.
//!
//! [`WireUnpacker`] is the byte-level twin for tools: it rewrites every packed
//! frame as the `M!{json}\n` line it stands for and passes every other byte
//! through untouched (`lp-cli wire unpack`). A capture decodes only from its
//! connection's start (or from the board's next reset); before that, the
//! unpacker names each frame it cannot read instead of guessing.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lp_json_pack::pack_learned::{HeaderMismatch, frame_epoch, is_reset_frame};
use lp_json_pack::{
    DecodeError, DropReason, LearnedTable, ScanEvent, VecFrameScanner, decode_learned,
    frame as cobs_frame, max_framed_len,
};

use crate::wire_encoding::{FRAME_KIND_LEARNED, WIRE_SEED};

/// The largest packed-frame body a host reader holds (COBS bytes). The
/// firmware's frame buffer is a few tens of KB; a body past this is a torn
/// frame swallowing console text, and is dropped rather than grown into.
pub const WIRE_STREAM_MAX_FRAME: usize = 256 * 1024;

/// One thing a board said, in stream order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireChunk {
    /// A console line that is not a wire message, with its `\n` (and a
    /// trailing `\r`) removed. Empty lines are delivered too.
    Line(String),
    /// One wire message's JSON.
    Frame(WireFrame),
    /// A packed frame that could not be delivered: torn, too long, of an
    /// unknown kind, or not decodable. Never dropped silently — a reader
    /// counts anomalies.
    Error(String),
    /// A packed frame dropped because this reader's learned table is not in
    /// step with the board's (see the module docs). The owner asks for a
    /// reset ([`PackOptIn::desynced`](crate::PackOptIn::desynced)) on every
    /// one; the opt-in's own rate limit keeps that to one request per
    /// interval.
    Desync(DesyncedFrame),
}

/// A packed frame this reader could not decode because its table was not in
/// step with the board's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesyncedFrame {
    /// The table epoch the frame names, when it is long enough to name one.
    pub epoch: Option<u8>,
    /// The frame's bytes on the wire (`0x00 'L' COBS 0x00`).
    pub wire_len: usize,
    /// Why, for logs: what the header said against what this reader holds.
    pub reason: String,
}

/// One wire message's JSON text and the form it came in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireFrame {
    /// The JSON, exactly as an `M!{json}` line would carry it (no `M!`, no
    /// newline). For a JSON line it is the line after `M!`, verbatim — a
    /// line with console text spliced into it is the caller's to resync.
    pub json: String,
    /// How it came over the link.
    pub form: WireForm,
}

/// How a wire message came over the link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireForm {
    /// An `M!{json}` line.
    Json,
    /// A packed frame of `wire_len` bytes: `0x00 'L' COBS 0x00`, not counting
    /// the `\n` before it (as a JSON line's size would not count the `\n`
    /// before its `M!`).
    Packed {
        /// The frame's bytes on the wire.
        wire_len: usize,
    },
}

impl WireFrame {
    /// Whether this message came packed.
    pub fn is_packed(&self) -> bool {
        matches!(self.form, WireForm::Packed { .. })
    }

    /// The `M!{json}` line this message is (or stands for), without a
    /// newline.
    pub fn to_line(&self) -> String {
        format!("M!{}", self.json)
    }

    /// The bytes the JSON form of this message takes on the wire:
    /// `M!{json}\n`.
    pub fn json_line_len(&self) -> usize {
        self.json.len() + 3
    }
}

/// Splits a board's byte stream into [`WireChunk`]s. See the module docs:
/// one per link, for the link's whole life.
pub struct WireStream {
    scanner: VecFrameScanner,
    /// Text since the last newline.
    text: Vec<u8>,
    reader: LearnedReader,
}

impl Default for WireStream {
    fn default() -> Self {
        Self::new()
    }
}

impl WireStream {
    /// A reader at the start of a stream.
    pub fn new() -> Self {
        Self {
            scanner: VecFrameScanner::with_max_body(WIRE_STREAM_MAX_FRAME),
            text: Vec::new(),
            reader: LearnedReader::new(),
        }
    }

    /// Feed bytes as they arrive (any split), calling `on` for each chunk
    /// they complete, in stream order.
    pub fn push(&mut self, bytes: &[u8], mut on: impl FnMut(WireChunk)) {
        let Self {
            scanner,
            text,
            reader,
        } = self;
        scanner.push(bytes, |event| match event {
            ScanEvent::Text(t) => {
                text.extend_from_slice(t);
                drain_lines(text, &mut on);
            }
            ScanEvent::Frame { kind, payload } => on(reader.frame(kind, payload)),
            ScanEvent::Dropped(reason) => on(WireChunk::Error(dropped_message(reason))),
        });
    }

    /// [`push`](Self::push), collected.
    pub fn push_collect(&mut self, bytes: &[u8]) -> Vec<WireChunk> {
        let mut chunks = Vec::new();
        self.push(bytes, |chunk| chunks.push(chunk));
        chunks
    }

    /// Forget a partial line or frame, and the learned table. Called on a
    /// (re)open or a link reset: what was half-read, and what was learned,
    /// belong to the previous port generation. If the board is still packed
    /// afterwards, its next frame reads as a [`WireChunk::Desync`] until the
    /// reader's owner has asked for a reset.
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    /// Bytes held back waiting for a newline, plus one if a frame is part
    /// read. A count that only grows is the mid-frame-cut signature.
    pub fn pending_bytes(&self) -> usize {
        self.text.len() + usize::from(self.scanner.in_frame())
    }
}

/// Emit every whole line in `text`, keeping the tail.
fn drain_lines(text: &mut Vec<u8>, on: &mut impl FnMut(WireChunk)) {
    let mut start = 0;
    while let Some(nl) = text[start..].iter().position(|&b| b == b'\n') {
        let mut line = &text[start..start + nl];
        if let Some(stripped) = line.strip_suffix(b"\r") {
            line = stripped;
        }
        on(line_chunk(line));
        start += nl + 1;
    }
    text.drain(..start);
}

fn line_chunk(line: &[u8]) -> WireChunk {
    let line = String::from_utf8_lossy(line);
    match line.strip_prefix("M!") {
        Some(json) => WireChunk::Frame(WireFrame {
            json: json.to_string(),
            form: WireForm::Json,
        }),
        None => WireChunk::Line(line.into_owned()),
    }
}

/// One link's learned table on the host side, and whether it is in step
/// with the board's.
struct LearnedReader {
    table: Box<LearnedTable>,
    /// False after a header mismatch: only the board's reset frame is
    /// decoded until then.
    in_step: bool,
}

impl LearnedReader {
    fn new() -> Self {
        Self {
            table: LearnedTable::boxed(),
            in_step: true,
        }
    }

    /// One scanned frame → its chunk, learning as the board did.
    fn frame(&mut self, kind: u8, payload: &[u8]) -> WireChunk {
        if kind != FRAME_KIND_LEARNED {
            return WireChunk::Error(format!(
                "a frame of unknown kind 0x{kind:02x} ({} bytes)",
                payload.len()
            ));
        }
        let wire_len = packed_wire_len(payload);
        if !self.in_step && !is_reset_frame(payload) {
            return self.desync(
                payload,
                wire_len,
                "waiting for the board's reset".to_string(),
            );
        }
        // Packed frames run 3-4x smaller than their JSON; start there.
        let mut out = Vec::with_capacity(payload.len() * 4);
        match decode_learned(&WIRE_SEED, &mut *self.table, payload, &mut out) {
            Ok(()) => {
                self.in_step = true;
                match String::from_utf8(out) {
                    Ok(json) => WireChunk::Frame(WireFrame {
                        json,
                        form: WireForm::Packed { wire_len },
                    }),
                    Err(_) => WireChunk::Error(
                        "packed frame decoded to text that is not UTF-8".to_string(),
                    ),
                }
            }
            Err(DecodeError::Learned(mismatch)) => {
                self.in_step = false;
                self.desync(payload, wire_len, mismatch_reason(mismatch))
            }
            // The header matched and the body is broken: a torn frame. The
            // table rolled back what it had learned from it; if the board
            // learned something there, the next frame's header says so.
            Err(error) => WireChunk::Error(format!("packed frame did not decode: {error:?}")),
        }
    }

    fn desync(&self, payload: &[u8], wire_len: usize, reason: String) -> WireChunk {
        WireChunk::Desync(DesyncedFrame {
            epoch: frame_epoch(payload),
            wire_len,
            reason,
        })
    }
}

fn mismatch_reason(mismatch: HeaderMismatch) -> String {
    match mismatch {
        HeaderMismatch::Truncated => "the frame is shorter than its header".to_string(),
        HeaderMismatch::Epoch { frame, reader } => {
            format!("the board is in table epoch {frame}, this reader in {reader}")
        }
        HeaderMismatch::State { frame, reader } => format!(
            "the board's table (state {frame:04x}) and this reader's ({reader:04x}) have parted"
        ),
    }
}

fn dropped_message(reason: DropReason) -> String {
    match reason {
        DropReason::BadCobs => "a packed frame was torn (its body is not valid COBS)".to_string(),
        DropReason::TooLong => {
            format!("a packed frame outgrew the reader's {WIRE_STREAM_MAX_FRAME} B buffer")
        }
    }
}

/// The bytes `payload` takes on the wire as a packed frame (`0x00 'L' COBS
/// 0x00`), counted by framing it again: exact, and only paid on hosts.
fn packed_wire_len(payload: &[u8]) -> usize {
    let mut out = alloc::vec![0u8; max_framed_len(payload.len())];
    cobs_frame(FRAME_KIND_LEARNED, payload, &mut out).unwrap_or(out.len())
}

/// One packed frame [`WireUnpacker`] rewrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnpackedFrame {
    /// The frame's bytes on the wire (`0x00 'L' COBS 0x00`).
    pub wire_len: usize,
    /// The bytes of the `M!{json}\n` line written in its place.
    pub json_line_len: usize,
}

/// What [`WireUnpacker`] did with one packed frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnpackEvent {
    /// Rewritten as its `M!{json}\n` line.
    Unpacked(UnpackedFrame),
    /// Not readable: the capture's table was not in step with the board's
    /// (the capture starts mid-connection, or a frame before it was lost).
    /// A marker line naming it was written in its place.
    Unreadable(DesyncedFrame),
    /// Dropped: torn, too long, of an unknown kind, or not decodable.
    Dropped(String),
}

/// Rewrites packed frames in a byte stream as the `M!{json}\n` lines they
/// stand for, passing every other byte through untouched.
///
/// For `lp-cli wire unpack`, which makes a capture readable to
/// line-oriented tools. Like [`WireStream`], it learns the link's table as it
/// goes, so it reads a capture **from its connection's start** (or from the
/// board's next table reset). A frame it cannot read is written as the line
/// `<learned frame: table unknown, epoch N, M bytes>` and reported as
/// [`UnpackEvent::Unreadable`]; a frame that cannot be delivered at all is
/// written as nothing and reported as [`UnpackEvent::Dropped`]. It never
/// guesses.
pub struct WireUnpacker {
    scanner: VecFrameScanner,
    reader: LearnedReader,
}

impl Default for WireUnpacker {
    fn default() -> Self {
        Self::new()
    }
}

impl WireUnpacker {
    /// An unpacker at the start of a stream.
    pub fn new() -> Self {
        Self {
            scanner: VecFrameScanner::with_max_body(WIRE_STREAM_MAX_FRAME),
            reader: LearnedReader::new(),
        }
    }

    /// Rewrite `bytes` onto `out`. `on_frame` hears what happened to each
    /// packed frame, in order.
    pub fn push(&mut self, bytes: &[u8], out: &mut Vec<u8>, mut on_frame: impl FnMut(UnpackEvent)) {
        let Self { scanner, reader } = self;
        scanner.push(bytes, |event| match event {
            ScanEvent::Text(t) => out.extend_from_slice(t),
            ScanEvent::Frame { kind, payload } => match reader.frame(kind, payload) {
                WireChunk::Frame(frame) => {
                    let WireForm::Packed { wire_len } = frame.form else {
                        unreachable!("a scanned frame is packed");
                    };
                    out.extend_from_slice(b"M!");
                    out.extend_from_slice(frame.json.as_bytes());
                    out.push(b'\n');
                    on_frame(UnpackEvent::Unpacked(UnpackedFrame {
                        wire_len,
                        json_line_len: frame.json_line_len(),
                    }));
                }
                WireChunk::Desync(desynced) => {
                    out.extend_from_slice(unreadable_marker(&desynced).as_bytes());
                    on_frame(UnpackEvent::Unreadable(desynced));
                }
                WireChunk::Error(error) => on_frame(UnpackEvent::Dropped(error)),
                WireChunk::Line(_) => unreachable!("a scanned frame is never a line"),
            },
            ScanEvent::Dropped(reason) => on_frame(UnpackEvent::Dropped(dropped_message(reason))),
        });
    }

    /// Whether a frame is part read (the stream ended inside one).
    pub fn in_frame(&self) -> bool {
        self.scanner.in_frame()
    }
}

/// The line a frame the unpacker cannot read is written as.
fn unreadable_marker(frame: &DesyncedFrame) -> String {
    match frame.epoch {
        Some(epoch) => format!(
            "<learned frame: table unknown, epoch {epoch}, {} bytes>\n",
            frame.wire_len
        ),
        None => format!("<learned frame: table unknown, {} bytes>\n", frame.wire_len),
    }
}

#[cfg(all(test, feature = "ser-write-json"))]
mod tests {
    use super::*;
    use crate::server::ServerMsgBody;
    use crate::{WireServerMessage, ser_learned_frame_to};
    use alloc::vec;
    use lp_json_pack::LearnStore;

    #[test]
    fn console_text_json_lines_and_packed_frames_come_out_in_order() {
        let a = message(3, "one");
        let b = message(4, "two\nlines");
        let stream = [
            b"[INIT] boot\r\n".to_vec(),
            json_line(&a),
            packed(&b),
            b"\ntail line\n".to_vec(),
        ]
        .concat();

        let chunks = WireStream::new().push_collect(&stream);

        assert_eq!(
            chunks,
            vec![
                WireChunk::Line("[INIT] boot".into()),
                // The `\n` a JSON line is written after.
                WireChunk::Line(String::new()),
                frame(&a, WireForm::Json),
                WireChunk::Line(String::new()),
                frame(&b, packed_form(&b)),
                WireChunk::Line(String::new()),
                WireChunk::Line("tail line".into()),
            ]
        );
    }

    /// A frame carries `0x0A` bytes as data, and a read can end anywhere.
    #[test]
    fn a_frame_split_at_every_offset_is_delivered_once_whole() {
        let msg = message(9, "a\nb\n\n\r\n");
        let bytes = [packed(&msg), b"after\n".to_vec()].concat();
        assert!(packed(&msg)[3..].contains(&b'\n'), "the frame holds a 0x0A");
        let expected = vec![
            frame(&msg, packed_form(&msg)),
            WireChunk::Line("after".into()),
        ];
        for cut in 0..=bytes.len() {
            let mut stream = WireStream::new();
            let mut chunks = stream.push_collect(&bytes[..cut]);
            chunks.extend(stream.push_collect(&bytes[cut..]));
            // The leading `\n` of the frame completes an empty line first.
            assert_eq!(chunks[0], WireChunk::Line(String::new()), "cut {cut}");
            assert_eq!(chunks[1..], expected[..], "cut {cut}");
        }
        // And byte by byte.
        let mut stream = WireStream::new();
        let chunks: Vec<_> = bytes
            .iter()
            .flat_map(|b| stream.push_collect(core::slice::from_ref(b)))
            .collect();
        assert_eq!(chunks[1..], expected[..]);
    }

    #[test]
    fn a_torn_frame_then_a_good_one() {
        let good = message(5, "good");
        let stream = [
            b"\n\x00P\x09abc".to_vec(),
            b"ESP-ROM:esp32c6\n".to_vec(),
            packed(&good),
        ]
        .concat();

        let chunks = WireStream::new().push_collect(&stream);

        assert!(
            chunks
                .iter()
                .any(|c| matches!(c, WireChunk::Error(e) if e.contains("torn"))),
            "{chunks:?}"
        );
        assert_eq!(
            chunks.last(),
            Some(&frame(&good, packed_form(&good))),
            "{chunks:?}"
        );
    }

    #[test]
    fn a_garbage_frame_is_an_error_not_silence() {
        let mut framed = [0u8; 16];
        // A header that matches a fresh table, then garbage.
        let n = cobs_frame(FRAME_KIND_LEARNED, &[0, 0, 0, 0xFF, 0xFE], &mut framed).unwrap();
        let chunks = WireStream::new().push_collect(&framed[..n]);
        assert!(
            matches!(chunks.as_slice(), [WireChunk::Error(e)] if e.contains("did not decode")),
            "{chunks:?}"
        );

        // The static-dictionary frame kind is gone: no dual decode.
        let n = cobs_frame(b'P', b"x", &mut framed).unwrap();
        let chunks = WireStream::new().push_collect(&framed[..n]);
        assert!(
            matches!(chunks.as_slice(), [WireChunk::Error(e)] if e.contains("unknown kind")),
            "{chunks:?}"
        );
    }

    #[test]
    fn clear_forgets_a_partial_line_and_a_partial_frame() {
        let msg = message(1, "x");
        let mut stream = WireStream::new();
        stream.push_collect(b"half a li");
        assert!(stream.pending_bytes() > 0);
        stream.clear();
        stream.push_collect(&packed(&msg)[..5]);
        assert!(stream.pending_bytes() > 0, "a frame part read is pending");
        stream.clear();
        assert_eq!(stream.pending_bytes(), 0);
        assert_eq!(
            stream.push_collect(b"whole\n"),
            vec![WireChunk::Line("whole".into())]
        );
    }

    #[test]
    fn the_unpacker_rewrites_frames_and_passes_every_other_byte_through() {
        let a = message(3, "one");
        let b = message(4, "two\nlines");
        let text = b"\xFFraw \x01 bytes\r\n".to_vec();
        let stream = [text.clone(), packed(&a), json_line(&b), b"tail".to_vec()].concat();
        let expected = [text, json_line(&a), json_line(&b), b"tail".to_vec()].concat();

        for step in [1, 2, 7, stream.len()] {
            let mut unpacker = WireUnpacker::new();
            let mut out = Vec::new();
            let mut frames = Vec::new();
            for chunk in stream.chunks(step) {
                unpacker.push(chunk, &mut out, |f| frames.push(f));
            }
            assert_eq!(out, expected, "step {step}");
            assert_eq!(
                frames,
                vec![UnpackEvent::Unpacked(UnpackedFrame {
                    wire_len: packed(&a).len() - 1,
                    json_line_len: json_line(&a).len() - 1,
                })],
                "step {step}"
            );
        }
    }

    /// A frame that changed the board's table is torn in flight: the next
    /// frame's header disagrees, it and every frame after it are dropped as
    /// desyncs (never decoded), and the board's reset frame brings the reader
    /// back.
    #[test]
    fn a_torn_frame_that_taught_the_board_desyncs_until_the_reset() {
        let mut board = Board::default();
        let mut stream = WireStream::new();
        let first = message(1, "first");
        assert_eq!(
            stream.push_collect(&board.frame(&first))[1..],
            [frame(
                &first,
                WireForm::Packed {
                    wire_len: board.last_len
                }
            )]
        );

        // Torn in flight. The board learned from it ("info" on its second
        // sighting, "torn" as a first sighting); the host rolls back.
        // Five bytes lost from its middle, as the real C6 link lost them.
        let mut torn = board.frame(&message(2, "torn"));
        let mid = torn.len() / 2;
        torn.drain(mid..mid + 5);
        let chunks = stream.push_collect(&torn);
        assert!(
            chunks.iter().any(|c| matches!(c, WireChunk::Error(_))),
            "{chunks:?}"
        );

        // The next frame names a table state the host does not hold.
        let taught = board.frame(&message(3, "later"));
        let chunks = stream.push_collect(&taught);
        let desynced = chunks
            .iter()
            .find_map(|c| match c {
                WireChunk::Desync(d) => Some(d.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{chunks:?}"));
        assert_eq!(desynced.epoch, Some(0));
        assert!(desynced.reason.contains("parted"), "{desynced:?}");

        // Still dropped, even a frame that happens to learn nothing.
        let chunks = stream.push_collect(&board.frame(&message(4, "later")));
        assert!(
            chunks.iter().any(|c| matches!(c, WireChunk::Desync(_))),
            "{chunks:?}"
        );

        // The board resets (it got the host's `SetEncoding`): back in step.
        board.table.reset(1);
        let after = message(5, "after the reset");
        let chunks = stream.push_collect(&board.frame(&after));
        assert_eq!(
            chunks[1..],
            [frame(
                &after,
                WireForm::Packed {
                    wire_len: board.last_len
                }
            )]
        );
        let again = message(6, "after the reset");
        assert!(matches!(
            stream.push_collect(&board.frame(&again))[1],
            WireChunk::Frame(_)
        ));
    }

    /// A reader that starts mid-connection, or was cleared while the board
    /// stayed packed, cannot read frames until the board resets.
    #[test]
    fn a_reader_cleared_mid_connection_waits_for_the_reset() {
        let mut board = Board::default();
        let mut stream = WireStream::new();
        stream.push_collect(&board.frame(&message(1, "a")));
        stream.clear();
        let chunks = stream.push_collect(&board.frame(&message(2, "b")));
        assert!(matches!(chunks[1], WireChunk::Desync(_)), "{chunks:?}");
        board.table.reset(1);
        let chunks = stream.push_collect(&board.frame(&message(3, "c")));
        assert!(matches!(chunks[1], WireChunk::Frame(_)), "{chunks:?}");
    }

    #[test]
    fn the_unpacker_names_the_frames_of_a_capture_that_starts_mid_connection() {
        let mut board = Board::default();
        board.frame(&message(1, "before the capture"));
        let unreadable = board.frame(&message(2, "b"));
        board.table.reset(7);
        let readable_msg = message(3, "c");
        let readable = board.frame(&readable_msg);
        let stream = [unreadable.clone(), readable].concat();

        let mut unpacker = WireUnpacker::new();
        let mut out = Vec::new();
        let mut events = Vec::new();
        unpacker.push(&stream, &mut out, |e| events.push(e));

        assert!(
            matches!(&events[..], [UnpackEvent::Unreadable(d), UnpackEvent::Unpacked(_)]
                if d.epoch == Some(0) && d.wire_len == unreadable.len() - 1),
            "{events:?}"
        );
        let text = String::from_utf8(out).unwrap();
        assert_eq!(
            text,
            format!(
                "\n<learned frame: table unknown, epoch 0, {} bytes>\n\nM!{}\n",
                unreadable.len() - 1,
                crate::json::to_string(&readable_msg).unwrap()
            )
        );
    }

    fn message(id: u64, text: &str) -> WireServerMessage {
        WireServerMessage::new(
            id,
            ServerMsgBody::Log {
                level: crate::server::api::LogLevel::Info,
                message: text.into(),
            },
        )
    }

    /// `\nM!{json}\n`, as the firmware writes a JSON reply.
    fn json_line(msg: &WireServerMessage) -> Vec<u8> {
        format!("\nM!{}\n", crate::json::to_string(msg).unwrap()).into_bytes()
    }

    /// `\n 0x00 'L' COBS 0x00`, as the firmware writes the first packed
    /// reply of a fresh link.
    fn packed(msg: &WireServerMessage) -> Vec<u8> {
        Board::default().frame(msg)
    }

    /// A board's side of one packed link.
    #[derive(Default)]
    struct Board {
        table: Box<LearnedTable>,
        last_len: usize,
    }

    impl Board {
        /// The next packed reply, as the firmware writes it; `last_len` is
        /// its wire length without the leading `\n`.
        fn frame(&mut self, msg: &WireServerMessage) -> Vec<u8> {
            let mut buf = vec![0u8; 4096];
            let n = ser_learned_frame_to(&mut buf, &mut *self.table, msg).unwrap();
            buf.truncate(n);
            self.last_len = n - 1;
            buf
        }
    }

    fn packed_form(msg: &WireServerMessage) -> WireForm {
        WireForm::Packed {
            wire_len: packed(msg).len() - 1,
        }
    }

    fn frame(msg: &WireServerMessage, form: WireForm) -> WireChunk {
        WireChunk::Frame(WireFrame {
            json: crate::json::to_string(msg).unwrap(),
            form,
        })
    }
}
