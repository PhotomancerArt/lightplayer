//! **Update protocol v1**: the messages of lp-link channel 3, one lp-link
//! message per protocol message, little-endian, the first byte the type.
//!
//! | Type | Direction | Layout (after the type byte) | Meaning |
//! |---|---|---|---|
//! | `Q` | host→board | `proto:u8` | "Who are you?" Answered with `M` |
//! | `M` | board→host | JSON of [`BoardManifest`](crate::BoardManifest) | The board manifest. Core-only also sends it unprompted when a link comes up |
//! | `O` | host→board | [`Offer`] | An offer of one build |
//! | `R` | board→host | [`Request`] | A request for one chunk |
//! | `D` | host→board | `kind:u8 off:u32 bytes…` | A raw chunk |
//! | `Z` | host→board | `kind:u8 off:u32 deflate…` | One chunk of encoding 1 |
//! | `G` | host→board | [`ReadBackRequest`] | Read-back request (kind `E` only in v1) |
//! | `D` | board→host | `kind:u8 off:u32 bytes…` | Read-back data (the direction tells the two `D`s apart) |
//! | `N` | board→host | [`Refusal`] | A refusal |
//! | `L` | both | [`HostLoginStep`] / [`BoardLoginStep`] | The core-side login |
//!
//! Kinds: `C` core, `E` engine ([`PieceKind`](crate::PieceKind)).
//!
//! # The additive-only rules (DM7, `one-way-doors.md` §5) — forever
//!
//! - **Unknown messages.** A board that receives a host message type it does
//!   not know answers `N`/`U` with the type byte, so a newer host learns at
//!   once that the board lacks it instead of timing out. A host ignores board
//!   message types it does not know (hosts are always the newer side). The
//!   decoders return `Unknown { ty }` for both; the board session answers it.
//! - **Trailing bytes.** A reader ignores bytes past the fields it knows; a
//!   message too short for them is an error; writers only ever append fields.
//! - **Flag bits** ([`crate::flag_rule`]): the low 4 bits of `O.flags` and
//!   `R.flags` may be ignored, the high 4 are must-understand.
//! - **`proto` in `Q`, `O` and `M` is information,** not a negotiation: a host
//!   speaks the board's version and never sends a newer message to an older
//!   board.
//! - Type letters and refusal reasons are never reused.

use alloc::vec::Vec;

use crate::chunk::{ChunkEncoding, ChunkRef};
use crate::login_step::{BoardLoginStep, HostLoginStep};
use crate::offer::Offer;
use crate::read_back_request::ReadBackRequest;
use crate::refusal::Refusal;
use crate::request::Request;
use crate::wire_reader::WireReader;

/// Why a message did not decode. Nothing here is a panic: a board drops a
/// message that does not decode and carries on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// No type byte at all.
    Empty,
    /// A known type, too short for its fields (or naming a kind that is not
    /// `C` or `E`).
    Malformed { ty: u8 },
}

/// A message a host sends, as a board decodes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostMessage<'a> {
    /// `Q`.
    Query { proto: u8 },
    /// `O`.
    Offer(Offer),
    /// `D` or `Z`.
    Chunk(ChunkRef<'a>),
    /// `G`.
    ReadBack(ReadBackRequest),
    /// `L` steps 0 and 2.
    Login(HostLoginStep),
    /// A type this reader does not know (or an `L` step it does not know):
    /// a board answers `N`/`U` with `ty`.
    Unknown { ty: u8 },
}

/// A message a board sends, as a host decodes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoardMessage<'a> {
    /// `M`: the manifest's JSON
    /// ([`BoardManifest::from_json`](crate::BoardManifest::from_json)).
    Manifest(&'a [u8]),
    /// `R`.
    Request(Request),
    /// `D`: read-back data.
    Data(ChunkRef<'a>),
    /// `N`.
    Refusal(Refusal),
    /// `L` steps 1 and 3.
    Login(BoardLoginStep),
    /// A type this reader does not know: a host ignores it.
    Unknown { ty: u8 },
}

impl<'a> HostMessage<'a> {
    /// Decode one host→board message.
    pub fn decode(bytes: &'a [u8]) -> Result<Self, DecodeError> {
        let (&ty, rest) = bytes.split_first().ok_or(DecodeError::Empty)?;
        let mut r = WireReader::new(rest);
        let bad = DecodeError::Malformed { ty };
        Ok(match ty {
            b'Q' => Self::Query {
                proto: r.u8().ok_or(bad)?,
            },
            b'O' => Self::Offer(Offer::decode(&mut r).ok_or(bad)?),
            b'D' => Self::Chunk(ChunkRef::decode(ChunkEncoding::Raw, r).ok_or(bad)?),
            b'Z' => Self::Chunk(ChunkRef::decode(ChunkEncoding::Encoding1, r).ok_or(bad)?),
            b'G' => Self::ReadBack(ReadBackRequest::decode(&mut r).ok_or(bad)?),
            b'L' => match HostLoginStep::decode(&mut r).ok_or(bad)? {
                Some(step) => Self::Login(step),
                None => Self::Unknown { ty },
            },
            _ => Self::Unknown { ty },
        })
    }

    /// The whole message, type byte first.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Query { proto } => alloc::vec![b'Q', *proto],
            Self::Offer(o) => o.encode(),
            Self::Chunk(c) => c.encode(),
            Self::ReadBack(g) => g.encode(),
            Self::Login(step) => step.encode(),
            Self::Unknown { ty } => alloc::vec![*ty],
        }
    }
}

impl<'a> BoardMessage<'a> {
    /// Decode one board→host message.
    pub fn decode(bytes: &'a [u8]) -> Result<Self, DecodeError> {
        let (&ty, rest) = bytes.split_first().ok_or(DecodeError::Empty)?;
        let mut r = WireReader::new(rest);
        let bad = DecodeError::Malformed { ty };
        Ok(match ty {
            b'M' => Self::Manifest(rest),
            b'R' => Self::Request(Request::decode(&mut r).ok_or(bad)?),
            b'D' => Self::Data(ChunkRef::decode(ChunkEncoding::Raw, r).ok_or(bad)?),
            b'N' => Self::Refusal(Refusal::decode(&mut r).ok_or(bad)?),
            b'L' => match BoardLoginStep::decode(&mut r).ok_or(bad)? {
                Some(step) => Self::Login(step),
                None => Self::Unknown { ty },
            },
            _ => Self::Unknown { ty },
        })
    }

    /// The whole message, type byte first.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Manifest(json) => {
                let mut out = Vec::with_capacity(1 + json.len());
                out.push(b'M');
                out.extend_from_slice(json);
                out
            }
            Self::Request(r) => r.encode(),
            Self::Data(c) => c.encode(),
            Self::Refusal(n) => n.encode(),
            Self::Login(step) => step.encode(),
            Self::Unknown { ty } => alloc::vec![*ty],
        }
    }
}

/// `Q` for protocol v1.
#[must_use]
pub fn encode_query(proto: u8) -> Vec<u8> {
    alloc::vec![b'Q', proto]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::piece_kind::PieceKind;

    #[test]
    fn unknown_types_decode_as_unknown_in_both_directions() {
        assert_eq!(
            HostMessage::decode(b"X123"),
            Ok(HostMessage::Unknown { ty: b'X' })
        );
        assert_eq!(
            BoardMessage::decode(b"Y"),
            Ok(BoardMessage::Unknown { ty: b'Y' })
        );
        assert_eq!(HostMessage::decode(&[]), Err(DecodeError::Empty));
    }

    #[test]
    fn trailing_bytes_are_ignored_and_short_messages_are_errors() {
        let r = Request {
            kind: PieceKind::Core,
            off: 8192,
            len: 4096,
            flags: 1,
        };
        let mut bytes = r.encode();
        bytes.extend_from_slice(b"future fields");
        assert_eq!(BoardMessage::decode(&bytes), Ok(BoardMessage::Request(r)));
        let short = &r.encode()[..10];
        assert_eq!(
            BoardMessage::decode(short),
            Err(DecodeError::Malformed { ty: b'R' })
        );
    }

    #[test]
    fn an_unknown_login_step_is_an_unknown_message() {
        assert_eq!(
            HostMessage::decode(&[b'L', 9]),
            Ok(HostMessage::Unknown { ty: b'L' })
        );
        // A board's step sent by a host is not a host step either.
        assert_eq!(
            HostMessage::decode(&[b'L', 3, 0, 0, 0, 0, 0]),
            Ok(HostMessage::Unknown { ty: b'L' })
        );
    }

    #[test]
    fn a_chunk_with_a_bad_kind_is_malformed() {
        assert_eq!(
            HostMessage::decode(&[b'D', b'X', 0, 0, 0, 0, 1, 2]),
            Err(DecodeError::Malformed { ty: b'D' })
        );
    }

    #[test]
    fn decoding_random_bytes_never_panics() {
        // A small deterministic generator: xorshift.
        let mut s: u32 = 0x9e37_79b9;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            s
        };
        let types = b"QMORDZGNLUX";
        for _ in 0..20_000 {
            let len = (next() % 200) as usize;
            let mut bytes: Vec<u8> = (0..len).map(|_| next() as u8).collect();
            if let Some(first) = bytes.first_mut() {
                *first = types[(next() as usize) % types.len()];
            }
            let _ = HostMessage::decode(&bytes);
            let _ = BoardMessage::decode(&bytes);
        }
    }
}
