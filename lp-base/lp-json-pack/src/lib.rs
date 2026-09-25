//! **JSON Pack**: a compact binary form of JSON that decodes back to
//! **byte-identical JSON text**, so nothing downstream of the decoder changes.
//!
//! LightPlayer's board sends its wire replies packed (the plan
//! `lp2025/2026-09-23-1701-lp-json-pack`; the format was built and measured as
//! "LPBJ" in the `ion-wire` spike). This crate is the generic codec. It knows
//! no wire vocabulary: the [`Dictionary`] of key names and common strings is
//! injected by the caller, and `lpc-wire` owns the wire's one.
//!
//! - `#![no_std]`, and no `alloc` on the device path (`default = []`, `lex`).
//! - [`PackEncoder`]: events in (`begin_map`, `key`, `str`, `u64`, `blob`, …),
//!   one frame out into a caller-owned buffer. Never panics; `Err(Full)`.
//! - [`PackLexer`] (feature `lex`): JSON text in, into the same encoder, so
//!   text and events mix in one frame.
//! - [`decode`]: one frame → the byte-identical JSON text through a
//!   [`JsonOut`] sink.
//! - [`cobs_frame`]: `0x00 'P' COBS(payload) 0x00` framing, in place.
//! - [`FrameScanner`]: a byte stream → text and decoded frames.
//! - Features: `lex`; `alloc` (`Vec` sinks and scanner, [`DictionaryBuilder`]);
//!   `std` (implies both, plus the `json-pack` tool).
//!
//! # The format
//!
//! One frame holds exactly one JSON value. There is no frame header: which
//! dictionary a frame is coded against is agreed outside it (on the wire, by
//! `WIRE_PROTO_VERSION` and a runtime check of [`Dictionary::fingerprint`]).
//!
//! Bytes are read in two positions. **Value position** is every value and
//! every array element:
//!
//! | byte | meaning |
//! |---|---|
//! | `00..=3F` | unsigned integer 0..=63 |
//! | `40..=7F` | value-dictionary string 0..=63 |
//! | `80..=9F` | inline string of 0..=31 bytes; the UTF-8 follows |
//! | `A0` | object: key position until `FF` |
//! | `A1` / `A2` | array start / end |
//! | `A3` `A4` `A5` | `null`, `false`, `true` |
//! | `A6` / `A7` | unsigned / negative integer: LEB128 magnitude |
//! | `A8` / `A9` | decimal, + / −: zigzag-LEB128 exponent, then LEB128 coefficient |
//! | `AA` | string: LEB128 length, then UTF-8 |
//! | `AB` | blob: LEB128 length, then raw bytes; decodes to padded standard base64 |
//! | `AC` | value-dictionary string 64 and up: LEB128 (index − 64) |
//! | `AD` | back-reference: LEB128 n, the n-th inline text or blob of this frame, as a string |
//! | `AE` | number text escape: LEB128 length, then the number's ASCII verbatim |
//! | `AF` | blob back-reference: LEB128 n, the n-th inline text or blob of this frame, as base64 |
//! | `B0..=FF` | unassigned |
//!
//! **Key position**, inside an object before each value:
//!
//! | byte | meaning |
//! |---|---|
//! | `00..=EF` | key-dictionary 0..=239 |
//! | `F0..=FB` + 1 byte `b` | key-dictionary 240 + ((tag − `F0`) << 8 \| `b`), up to 3,311 |
//! | `FC` | inline key: LEB128 length, then UTF-8 |
//! | `FD` | back-reference: LEB128 n, the n-th inline text or blob of this frame |
//! | `FE` | unassigned |
//! | `FF` | end of object |
//!
//! Details:
//! - **Varints** are LEB128: seven bits a byte, low group first, high bit set
//!   on every byte but the last ([`pack_varint`]).
//! - **Decimals are text-exact** ([`pack_decimal`]). A float arrives as the
//!   text `ryu-js` printed (JS `Number.prototype.toString` layout). Its digits
//!   become an integer coefficient and its point a base-10 exponent, and the
//!   decoder lays them out again with the JS rules. The encoder takes this form
//!   only when that layout reproduces the text byte for byte; any other number
//!   text travels verbatim behind `AE`. Integers that fit a u64 are integers
//!   (`-0` is a decimal).
//! - **Containers are end-delimited**: no lengths to back-patch, so both
//!   encoders stream forward.
//! - **Strings** are, in order of preference: a dictionary code; a
//!   back-reference to an earlier inline text of the same frame; inline, with
//!   the length in the tag when it is 31 bytes or fewer. Blobs are a
//!   back-reference when the frame already carried the same bytes, and raw
//!   otherwise.
//! - **Back-references** number every inline text (value `80..=9F` and `AA`,
//!   key `FC`) and every blob (`AB`) of 2 to 65,535 bytes in order of
//!   appearance, all in one sequence, up to [`MAX_BACKREFS`] per frame.
//!   Numbers, back-references and dictionary hits are not numbered. The
//!   referring tag decides how the bytes print: `AD` and `FD` as a string,
//!   `AF` as base64.
//! - **Blobs** are bytes the JSON carries as base64. The encoder is told a
//!   value is a blob (by the type that serialized it); it never guesses from a
//!   string's shape. The decoder prints canonical padded standard base64.
//! - **Output text** is compact JSON, strings escaped exactly as
//!   `ser-write-json` does: `"` and `\` escaped, `\b \t \n \f \r` short, the other
//!   C0 controls as upper-case `\u00XX`, everything else (including non-ASCII
//!   UTF-8) as is.
//!
//! # Framing
//!
//! A frame on a byte stream that also carries console text is
//! `0x00 'P' COBS(payload) 0x00` ([`cobs_frame`], [`FRAME_KIND_PACK`]). NUL
//! never appears in console text nor in COBS output, so a text reader meeting
//! `0x00` knows a frame starts. Overhead is 3 bytes plus 1 per 254. The payload
//! may contain `0x0A`, so line splitters must find frames first
//! ([`FrameScanner`]).
//!
//! # Deviations from Ion
//!
//! JSON Pack is Ion-inspired (a shared symbol table, typed binary values) but
//! not Ion. Each deviation was measured as one rung of an ablation ladder over
//! a real PLAYFUL-choker lens session (exact medians of a lens reply, bytes,
//! today → after the send-less change; spike findings, 2026-09-23). The model's
//! last rung equalled the Rust prototype byte for byte on 2,270 of 2,270
//! frames.
//!
//! | rung | change | lens reply | after send-less | why deviate |
//! |---|---|---:|---:|---|
//! | R0 | the JSON line | 11,095 | 3,748 | |
//! | R1 | strict Ion 1.0, shared table imported per frame, blobs raw, float32 | 3,063 | 1,103 | almost all of the win is here |
//! | R2 | no per-frame version marker + symbol-table header (31 B) | 3,032 | 1,072 | the hello's `WIRE_PROTO_VERSION` already names the table; saves 31 B on every frame (a knob turn 84 → 53) |
//! | R3 | text-exact decimals instead of float32 | 3,038 | 1,076 | +4 B, but no float parser on the device and the exact text back |
//! | R4 | end-delimited containers instead of length prefixes | 3,185 | 1,114 | **costs 3.5–5 %**: the one deviation paid for in bytes, kept because a streaming encoder with length prefixes back-patches (memmoves) every container |
//! | R5 | separate key and value tables; 240 one-byte keys (Ion: SIDs < 128) | 3,147 | 1,085 | −1–3 % |
//! | R6 | small integers and the 64 commonest values in the tag byte; LEB128 | 2,874 | 1,024 | −6–9 %, the biggest deviation win |
//! | R7 | inline strings to 31 B with the length in the tag (Ion: 13) | 2,874 | 1,024 | ±0 here |
//! | R8 | per-frame back-references | 2,874 | 1,024 | ±0 on the median; it pays on a repeated blob (the device's doubled `layout2d` payload). Blobs joined the table in the product (`AF`): in the pre-lean-wire Run D sample, 9.9 KB of 20.4 KB of base64 repeated within its own frame |
//!
//! What is given up: an off-the-shelf Ion reader cannot read a capture, and
//! there is no external reference implementation. The decoder here (and the
//! `json-pack` tool) turns any capture back into JSON.

#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod cobs_frame;
pub mod frame_scanner;
pub mod pack_base64;
pub mod pack_decimal;
pub mod pack_decoder;
pub mod pack_dictionary;
#[cfg(feature = "alloc")]
pub mod pack_dictionary_builder;
pub mod pack_encoder;
pub mod pack_learned;
#[cfg(feature = "lex")]
pub mod pack_lexer;
pub mod pack_tags;
pub mod pack_varint;

pub use cobs_frame::{
    FRAME_DELIMITER, FRAME_KIND_PACK, frame, frame_in_place, in_place_headroom, max_framed_len,
};
pub use frame_scanner::{DropReason, FrameBuffer, FrameScanner, ScanEvent, SliceFrameBuffer};
#[cfg(feature = "alloc")]
pub use frame_scanner::{VecFrameBuffer, VecFrameScanner};
pub use pack_decoder::{DecodeError, JsonOut, JsonOutFull, SliceJsonOut, decode, decode_learned};
pub use pack_dictionary::{Dictionary, DictionaryError, PACK_FORMAT_VERSION, PackStrings};
#[cfg(feature = "alloc")]
pub use pack_dictionary_builder::{DictionaryBuilder, OwnedDictionary};
pub use pack_encoder::PackEncoder;
pub use pack_learned::{HeaderMismatch, LearnMark, LearnStore, LearnedTable};
#[cfg(feature = "lex")]
pub use pack_lexer::PackLexer;

/// Back-reference table size per frame, shared by encoder and decoder.
pub const MAX_BACKREFS: usize = 64;

/// Why a frame could not be encoded. Either way the caller sends JSON instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackError {
    /// The output buffer is full.
    Full,
    /// The input is not JSON this codec reproduces byte for byte.
    Malformed,
}

/// Whether an inline text of `len` bytes is numbered for back-references.
/// Encoder and decoder must agree on this exactly.
pub(crate) fn is_backref_candidate(len: usize) -> bool {
    (2..=usize::from(u16::MAX)).contains(&len)
}
