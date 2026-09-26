//! The packed sink: the wire serializer's tokens into a JSON Pack frame.
//!
//! [`PackSink`] is a [`SerWrite`] that takes every structural token the
//! vendored `ser-write-json` offers (`TAKES_TOKENS`) and turns it into an
//! `lp-json-pack` event, so a wire type packs through the **same** serializer
//! instantiation that writes its JSON (see
//! [`ser_write_json_to`](crate::ser_write_json_to)). Nothing is printed and
//! re-lexed on this path, except:
//!
//! - **floats**, printed with `ryu-js` exactly as `ser-write-json` prints them
//!   and handed over as decimal text, so the packed decimal decodes to the
//!   same bytes the JSON would have carried;
//! - **text the serializer writes without a token**: a `RawValue` (slot data,
//!   pre-serialized JSON written verbatim) and `collect_str` strings. That
//!   text goes through the `lex` lexer into the same frame, sharing its
//!   back-reference table.
//!
//! A blob (`serde_base64` fields, recognized by type through the `$lp::blob`
//! marker) arrives as [`Token::Blob`] and travels as raw bytes.
//!
//! One limit: a map *key* serialized with `collect_str` would arrive as text
//! in key position and pack as a value. No wire type has such a key (keys are
//! field names, variant names and `String`s); the corpus test over recorded
//! traffic would catch one.

use lp_json_pack::{LearnStore, PackEncoder, PackError, PackLexer};
use ser_write_json::SerWrite;
use ser_write_json::ser_write::Token;

use crate::wire_encoding::WIRE_SEED;

/// A [`SerWrite`] that packs what the wire serializer offers into a
/// caller-owned buffer. Errors are sticky: after `Full` every later call
/// fails the same way.
pub struct PackSink<'a> {
    enc: PackEncoder<'a>,
    lexer: PackLexer,
    /// Whether text has been fed to `lexer` since the last token: that text is
    /// one complete JSON value, finished at the next token or at the end.
    lexing: bool,
}

impl<'a> PackSink<'a> {
    /// A sink writing one learned frame (header first) from the start of
    /// `out`, coded against the link's `table`, which learns as the frame is
    /// written. The caller rolls `table` back if the frame is not sent.
    pub fn new(out: &'a mut [u8], table: &'a mut dyn LearnStore) -> Self {
        Self {
            enc: PackEncoder::with_learned(out, &WIRE_SEED, table),
            lexer: PackLexer::new(),
            lexing: false,
        }
    }

    /// Bytes written so far.
    pub fn len(&self) -> usize {
        self.enc.len()
    }

    /// Whether nothing has been written.
    pub fn is_empty(&self) -> bool {
        self.enc.is_empty()
    }

    /// The frame's length, or the first error: [`PackError::Full`] when the
    /// buffer was too small, [`PackError::Malformed`] when written text was
    /// not JSON the codec reproduces byte for byte. Either way, send JSON.
    pub fn finish(mut self) -> Result<usize, PackError> {
        self.end_text()?;
        self.enc.finish()
    }

    /// Close the lexed value in progress, if any.
    fn end_text(&mut self) -> Result<(), PackError> {
        if self.lexing {
            self.lexing = false;
            self.lexer.finish(&mut self.enc)?;
            self.lexer = PackLexer::new();
        }
        Ok(())
    }

    fn event(&mut self, token: Token<'_>) -> Result<(), PackError> {
        let enc = &mut self.enc;
        match token {
            Token::Str(s) => enc.str(s),
            Token::Key(k) => enc.key(k),
            Token::U64(v) => enc.u64(v),
            Token::I64(v) => enc.i64(v),
            Token::F32(v) => enc.decimal_text(ryu_js::Buffer::new().format_finite(v)),
            Token::F64(v) => enc.decimal_text(ryu_js::Buffer::new().format_finite(v)),
            Token::Bool(v) => enc.bool(v),
            Token::Null => enc.null(),
            Token::MapBegin => enc.begin_map(),
            Token::MapEnd => enc.end_map(),
            Token::SeqBegin => enc.begin_seq(),
            Token::SeqEnd => enc.end_seq(),
            // Punctuation: the packed form has none.
            Token::Separator => Ok(()),
            Token::Blob(bytes) => enc.blob(bytes),
            // A token this sink does not know. Declining it would put its
            // text between taken tokens, which no lexer can pair up, so the
            // frame is refused and goes as JSON.
            _ => Err(PackError::Malformed),
        }
    }
}

impl SerWrite for PackSink<'_> {
    type Error = PackError;

    const TAKES_TOKENS: bool = true;

    fn write(&mut self, buf: &[u8]) -> Result<(), PackError> {
        self.lexing = true;
        self.lexer.push(&mut self.enc, buf)
    }

    fn token(&mut self, token: Token<'_>) -> Result<bool, PackError> {
        self.end_text()?;
        self.event(token)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ser_write::{WireWriteError, ser_learned_to};
    use crate::slot::{WireSlotData, WireSlotRootSnapshot};
    use crate::test_traffic::{TrafficDirection, TrafficLine, traffic_lines};
    use crate::{
        ClientMessage, ProjectReadEvent, ProjectReadNodeEvent, ProjectReadQueryEvent,
        WireServerMessage, ser_write_json_to,
    };
    use alloc::string::{String, ToString};
    use alloc::vec;
    use alloc::vec::Vec;
    use lp_json_pack::{LearnedTable, decode_learned};
    use lpc_model::{Revision, SlotShapeId};

    /// The corpus through the real types, in order, through one board table
    /// and one host table: every recorded line parses, packs through
    /// `PackSink`, and decodes back to the byte-identical line.
    #[test]
    fn recorded_traffic_packs_and_decodes_byte_for_byte() {
        let mut buf = vec![0u8; 1 << 16];
        let mut board = LearnedTable::NEW;
        let mut host = LearnedTable::NEW;
        let (mut json_total, mut packed_total, mut lines) = (0, 0, 0);
        for line in traffic_lines() {
            let n = pack_line(&line, &mut board, &mut buf).unwrap();
            assert_eq!(
                decode(&mut host, &buf[..n]),
                line.json,
                "line {}",
                line.index
            );
            json_total += line.json.len();
            packed_total += n;
            lines += 1;
        }
        assert!(lines > 100, "{lines} lines");
        assert_eq!(board.mark(), host.mark());
        // A ratchet. The static dictionary this replaced packed the proto-22
        // sample to 31.9 %; learned, a name costs its inline bytes once.
        assert!(
            packed_total * 25 < json_total * 9,
            "{packed_total} vs {json_total}"
        );
    }

    /// Serialized through the lexer instead (the JSON text of each line), a
    /// frame packs to the same bytes as the token path — with blobs declined,
    /// since the lexer cannot know a string is base64.
    #[test]
    fn the_token_path_packs_what_the_lexer_packs() {
        let mut by_tokens = vec![0u8; 1 << 16];
        let mut by_lexer = vec![0u8; 1 << 16];
        let mut a_table = LearnedTable::NEW;
        let mut b_table = LearnedTable::NEW;
        for line in traffic_lines() {
            let mut sink = NoBlobs(PackSink::new(&mut by_tokens, &mut a_table));
            serialize_line(&line, &mut sink);
            let a = sink.0.finish().unwrap();
            let mut enc = PackEncoder::with_learned(&mut by_lexer, &WIRE_SEED, &mut b_table);
            enc.json_value(line.json.as_bytes()).unwrap();
            let b = enc.finish().unwrap();
            assert_eq!(&by_tokens[..a], &by_lexer[..b], "line {}", line.index);
        }
    }

    #[test]
    fn a_raw_value_mixes_with_tokens_in_one_frame() {
        // Slot data is pre-serialized JSON written verbatim: lexed, into the
        // frame the typed envelope's tokens are building, back-references and
        // all.
        let raw = r#"{"zz_unknown_key":"a repeated slot value","gain":1.5,"xs":[1,-2,3.25e-7],"again":"a repeated slot value"}"#;
        let event = ProjectReadEvent::Query {
            index: 1,
            event: ProjectReadQueryEvent::Nodes(ProjectReadNodeEvent::SlotRoot(
                WireSlotRootSnapshot {
                    name: "a repeated slot value".to_string(),
                    shape: SlotShapeId::new(7),
                    data: WireSlotData::from_json_string(raw.to_string()).unwrap(),
                },
            )),
        };
        let msg = WireServerMessage::stream_frame(
            9,
            2,
            true,
            crate::server::ServerMsgBody::ProjectRead {
                events: vec![
                    ProjectReadEvent::Begin {
                        revision: Revision::new(4),
                    },
                    event,
                ],
            },
        );
        let json = crate::json::to_string(&msg).unwrap();
        assert!(json.contains(raw));
        let mut buf = [0u8; 512];
        let mut board = LearnedTable::NEW;
        let n = ser_learned_to(&mut buf, &mut board, &msg).unwrap();
        assert_eq!(decode(&mut LearnedTable::default(), &buf[..n]), json);
        // The name and the raw value's two copies became one inline text and
        // two back-references.
        let text = b"a repeated slot value";
        let copies = buf[..n].windows(text.len()).filter(|w| w == text).count();
        assert_eq!(copies, 1);
        // The raw value's keys were learned like any other.
        assert!(board.find_key(b"zz_unknown_key").is_some());
    }

    #[test]
    fn a_short_buffer_is_full_never_a_panic_and_learns_nothing() {
        let line = traffic_lines()
            .filter(|l| l.direction == TrafficDirection::BoardToHost)
            .max_by_key(|l| l.json.len())
            .unwrap();
        let mut buf = vec![0u8; 1 << 16];
        let n = pack_line(&line, &mut LearnedTable::default(), &mut buf).unwrap();
        for size in 0..n {
            let mut short = vec![0u8; size];
            let mut table = LearnedTable::NEW;
            assert_eq!(
                pack_line(&line, &mut table, &mut short),
                Err(WireWriteError::Full),
                "size {size} of {n}"
            );
            assert_eq!(table.mark(), LearnedTable::NEW.mark(), "size {size}");
        }
    }

    #[test]
    fn text_the_codec_cannot_reproduce_is_unpackable_and_learns_nothing() {
        // Whitespace inside a RawValue: valid JSON, but not byte-identical
        // after a round trip, so the lexer refuses it and the frame goes JSON.
        let data = WireSlotData::from_json_string(r#"{"a": 1}"#.to_string()).unwrap();
        let mut buf = [0u8; 64];
        let mut table = LearnedTable::NEW;
        assert_eq!(
            ser_learned_to(&mut buf, &mut table, &data),
            Err(WireWriteError::Unpackable)
        );
        assert_eq!(table.mark(), LearnedTable::NEW.mark());
    }

    /// What learning costs and buys over the sample: the first sighting of
    /// a name is inline, the rest are codes, so later replies of a class are
    /// smaller than the first.
    #[test]
    fn a_repeated_reply_class_packs_smaller_the_second_time() {
        let mut buf = vec![0u8; 1 << 16];
        let mut table = LearnedTable::NEW;
        let heartbeats: Vec<usize> = traffic_lines()
            .filter(|l| l.json.contains(r#""heartbeat":"#))
            .map(|l| pack_line(&l, &mut table, &mut buf).unwrap())
            .collect();
        assert!(heartbeats.len() > 3, "{heartbeats:?}");
        let (first, steady) = (heartbeats[0], heartbeats[heartbeats.len() - 1]);
        assert!(steady * 2 < first, "{heartbeats:?}");
    }

    fn decode(host: &mut LearnedTable, packed: &[u8]) -> String {
        let mut out = Vec::new();
        decode_learned(&WIRE_SEED, host, packed, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    /// Declines blobs, so they are written as base64 text and lexed.
    struct NoBlobs<'a>(PackSink<'a>);

    impl SerWrite for NoBlobs<'_> {
        type Error = PackError;
        const TAKES_TOKENS: bool = true;

        fn write(&mut self, buf: &[u8]) -> Result<(), PackError> {
            self.0.write(buf)
        }

        fn token(&mut self, token: Token<'_>) -> Result<bool, PackError> {
            if matches!(token, Token::Blob(_)) {
                return Ok(false);
            }
            self.0.token(token)
        }
    }

    fn serialize_line<W: SerWrite>(line: &TrafficLine, sink: &mut W) {
        match line.direction {
            TrafficDirection::BoardToHost => {
                let msg: WireServerMessage = crate::json::from_str(line.json).unwrap();
                ser_write_json_to(sink, &msg).unwrap();
            }
            TrafficDirection::HostToBoard => {
                let msg: ClientMessage = crate::json::from_str(line.json).unwrap();
                ser_write_json_to(sink, &msg).unwrap();
            }
        }
    }

    fn pack_line(
        line: &TrafficLine,
        table: &mut LearnedTable,
        buf: &mut [u8],
    ) -> Result<usize, WireWriteError> {
        match line.direction {
            TrafficDirection::BoardToHost => {
                let msg: WireServerMessage = crate::json::from_str(line.json).unwrap();
                ser_learned_to(buf, table, &msg)
            }
            TrafficDirection::HostToBoard => {
                let msg: ClientMessage = crate::json::from_str(line.json).unwrap();
                ser_learned_to(buf, table, &msg)
            }
        }
    }
}
