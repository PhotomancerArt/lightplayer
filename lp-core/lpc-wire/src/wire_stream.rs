//! Hosts: a board's byte stream → console lines and wire messages, in either
//! encoding.
//!
//! A board writes three things onto one byte stream: console text, `M!{json}`
//! lines, and — on a link that opted in (plan `lp-json-pack`, Q1) — packed
//! frames, `\n 0x00 'P' COBS(packed) 0x00`
//! ([`ser_packed_frame_to`](crate::ser_packed_frame_to)). A packed frame may
//! hold any byte, `\n` included, so a plain line splitter tears it; this one
//! finds frames first ([`lp_json_pack::FrameScanner`]) and splits only the
//! text between them into lines.
//!
//! Every host reader goes through [`WireStream`], and every reader must accept
//! **both** forms at all times: a board resets its link to JSON when the cable
//! is pulled, when it reboots, and when the host stops draining for a while,
//! so a link agreed packed can fall back to JSON mid-session. A packed frame
//! decodes (against [`WIRE_DICTIONARY`](crate::WIRE_DICTIONARY)) to exactly
//! the JSON text its `M!` line would have carried, so the caller handles
//! [`WireChunk::Frame`] the same way whichever form it came in.
//!
//! [`WireUnpacker`] is the byte-level twin for tools: it rewrites every packed
//! frame as the `M!{json}\n` line it stands for and passes every other byte
//! through untouched (`lp-cli wire unpack`).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lp_json_pack::{
    DropReason, FRAME_KIND_PACK, ScanEvent, VecFrameScanner, frame as cobs_frame, max_framed_len,
};

use crate::packed_json_decode::decode_packed_to_json;

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
    /// A packed frame of `wire_len` bytes: `0x00 'P' COBS 0x00`, not counting
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

/// Splits a board's byte stream into [`WireChunk`]s. See the module docs.
pub struct WireStream {
    scanner: VecFrameScanner,
    /// Text since the last newline.
    text: Vec<u8>,
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
        }
    }

    /// Feed bytes as they arrive (any split), calling `on` for each chunk
    /// they complete, in stream order.
    pub fn push(&mut self, bytes: &[u8], mut on: impl FnMut(WireChunk)) {
        let Self { scanner, text } = self;
        scanner.push(bytes, |event| match event {
            ScanEvent::Text(t) => {
                text.extend_from_slice(t);
                drain_lines(text, &mut on);
            }
            ScanEvent::Frame { kind, payload } => on(packed_chunk(kind, payload)),
            ScanEvent::Dropped(reason) => on(WireChunk::Error(dropped_message(reason))),
        });
    }

    /// [`push`](Self::push), collected.
    pub fn push_collect(&mut self, bytes: &[u8]) -> Vec<WireChunk> {
        let mut chunks = Vec::new();
        self.push(bytes, |chunk| chunks.push(chunk));
        chunks
    }

    /// Forget a partial line or frame. Called on a (re)open or a reset: what
    /// was half-read belongs to the previous port generation.
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

fn packed_chunk(kind: u8, payload: &[u8]) -> WireChunk {
    if kind != FRAME_KIND_PACK {
        return WireChunk::Error(format!(
            "a frame of unknown kind 0x{kind:02x} ({} bytes)",
            payload.len()
        ));
    }
    match decode_packed_to_json(payload) {
        Ok(json) => WireChunk::Frame(WireFrame {
            json,
            form: WireForm::Packed {
                wire_len: packed_wire_len(payload),
            },
        }),
        Err(error) => WireChunk::Error(error.to_string()),
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

/// The bytes `payload` takes on the wire as a packed frame (`0x00 'P' COBS
/// 0x00`), counted by framing it again: exact, and only paid on hosts.
fn packed_wire_len(payload: &[u8]) -> usize {
    let mut out = alloc::vec![0u8; max_framed_len(payload.len())];
    cobs_frame(FRAME_KIND_PACK, payload, &mut out).unwrap_or(out.len())
}

/// One packed frame [`WireUnpacker`] rewrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnpackedFrame {
    /// The frame's bytes on the wire (`0x00 'P' COBS 0x00`).
    pub wire_len: usize,
    /// The bytes of the `M!{json}\n` line written in its place.
    pub json_line_len: usize,
}

/// Rewrites packed frames in a byte stream as the `M!{json}\n` lines they
/// stand for, passing every other byte through untouched.
///
/// A frame that cannot be delivered is written as nothing and reported to
/// the caller as an error. For `lp-cli wire unpack`, which makes a capture
/// readable to line-oriented tools.
pub struct WireUnpacker {
    scanner: VecFrameScanner,
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
        }
    }

    /// Rewrite `bytes` onto `out`. `on_frame` hears each rewritten frame, in
    /// order, as `Ok` or as the reason it was dropped.
    pub fn push(
        &mut self,
        bytes: &[u8],
        out: &mut Vec<u8>,
        mut on_frame: impl FnMut(Result<UnpackedFrame, String>),
    ) {
        self.scanner.push(bytes, |event| match event {
            ScanEvent::Text(t) => out.extend_from_slice(t),
            ScanEvent::Frame { kind, payload } => match packed_chunk(kind, payload) {
                WireChunk::Frame(frame) => {
                    let WireForm::Packed { wire_len } = frame.form else {
                        unreachable!("a scanned frame is packed");
                    };
                    out.extend_from_slice(b"M!");
                    out.extend_from_slice(frame.json.as_bytes());
                    out.push(b'\n');
                    on_frame(Ok(UnpackedFrame {
                        wire_len,
                        json_line_len: frame.json_line_len(),
                    }));
                }
                WireChunk::Error(error) => on_frame(Err(error)),
                WireChunk::Line(_) => unreachable!("a scanned frame is never a line"),
            },
            ScanEvent::Dropped(reason) => on_frame(Err(dropped_message(reason))),
        });
    }

    /// Whether a frame is part read (the stream ended inside one).
    pub fn in_frame(&self) -> bool {
        self.scanner.in_frame()
    }
}

#[cfg(all(test, feature = "ser-write-json"))]
mod tests {
    use super::*;
    use crate::server::ServerMsgBody;
    use crate::{WireServerMessage, ser_packed_frame_to};
    use alloc::vec;

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
        let n = cobs_frame(FRAME_KIND_PACK, &[0xFF, 0xFE, 0xFD], &mut framed).unwrap();
        let chunks = WireStream::new().push_collect(&framed[..n]);
        assert!(
            matches!(chunks.as_slice(), [WireChunk::Error(e)] if e.contains("did not decode")),
            "{chunks:?}"
        );

        let n = cobs_frame(b'Q', b"x", &mut framed).unwrap();
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
                vec![Ok(UnpackedFrame {
                    wire_len: packed(&a).len() - 1,
                    json_line_len: json_line(&a).len() - 1,
                })],
                "step {step}"
            );
        }
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

    /// `\n 0x00 'P' COBS 0x00`, as the firmware writes a packed reply.
    fn packed(msg: &WireServerMessage) -> Vec<u8> {
        let mut buf = vec![0u8; 4096];
        let n = ser_packed_frame_to(&mut buf, msg).unwrap();
        buf.truncate(n);
        buf
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
