//! `D` and `Z`: one chunk of a piece.
//!
//! ```text
//! 'D' kind:u8 off:u32 bytes…     raw bytes (host→board), or read-back data (board→host)
//! 'Z' kind:u8 off:u32 deflate…   encoding 1 (host→board only)
//! ```
//!
//! A host's `D` and a board's read-back `D` have the same shape; the
//! direction tells them apart. A `Z` is one chunk of encoding 1: an
//! independent raw-deflate stream compressed against the dictionary the
//! dictionary rule names ([`crate::dictionary_rule`]), sent only in answer to
//! a request that set flag bit 0.
//!
//! Decoding borrows the payload: a board writes it straight to flash, with no
//! allocation per chunk.

use alloc::vec::Vec;

use crate::piece_kind::PieceKind;
use crate::wire_reader::WireReader;

/// How a chunk's payload is carried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkEncoding {
    /// `D`: the bytes themselves.
    Raw,
    /// `Z`: encoding 1.
    Encoding1,
}

impl ChunkEncoding {
    /// The message type byte.
    #[must_use]
    pub const fn type_byte(self) -> u8 {
        match self {
            Self::Raw => b'D',
            Self::Encoding1 => b'Z',
        }
    }
}

/// A decoded `D` or `Z`, borrowing its payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkRef<'a> {
    pub encoding: ChunkEncoding,
    pub kind: PieceKind,
    pub off: u32,
    pub payload: &'a [u8],
}

impl<'a> ChunkRef<'a> {
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        encode_chunk(self.encoding, self.kind, self.off, self.payload)
    }

    pub(crate) fn decode(encoding: ChunkEncoding, mut r: WireReader<'a>) -> Option<Self> {
        let kind = PieceKind::from_byte(r.u8()?)?;
        let off = r.u32()?;
        Some(Self {
            encoding,
            kind,
            off,
            payload: r.rest(),
        })
    }
}

/// Encode a `D` or `Z` without building a [`ChunkRef`] first.
#[must_use]
pub fn encode_chunk(encoding: ChunkEncoding, kind: PieceKind, off: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(6 + payload.len());
    out.push(encoding.type_byte());
    out.push(kind.byte());
    out.extend_from_slice(&off.to_le_bytes());
    out.extend_from_slice(payload);
    out
}
