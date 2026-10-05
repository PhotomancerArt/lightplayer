//! `R`, board→host: a request for one chunk.
//!
//! ```text
//! 'R' kind:u8 off:u32 len:u32 flags:u8
//! ```
//!
//! `off` is the chunk's byte offset in the piece and `len` its length (a
//! [`crate::code_table::CHUNK`], or the piece's short tail). Flag bit 0
//! ([`crate::flag_rule::REQUEST_TAKES_ENCODING_1`]) says this board takes
//! encoding 1 (`Z`) for it; a request without it always gets `D` — that is
//! the board's raw fallback after a `Z` that did not decode.

use alloc::vec::Vec;

use crate::flag_rule::REQUEST_TAKES_ENCODING_1;
use crate::piece_kind::PieceKind;
use crate::wire_reader::WireReader;

/// The `R` message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    pub kind: PieceKind,
    pub off: u32,
    pub len: u32,
    pub flags: u8,
}

impl Request {
    /// Whether the board takes encoding 1 for this chunk.
    #[must_use]
    pub const fn takes_encoding_1(&self) -> bool {
        self.flags & REQUEST_TAKES_ENCODING_1 != 0
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(11);
        out.push(b'R');
        out.push(self.kind.byte());
        out.extend_from_slice(&self.off.to_le_bytes());
        out.extend_from_slice(&self.len.to_le_bytes());
        out.push(self.flags);
        out
    }

    pub(crate) fn decode(r: &mut WireReader<'_>) -> Option<Self> {
        Some(Self {
            kind: PieceKind::from_byte(r.u8()?)?,
            off: r.u32()?,
            len: r.u32()?,
            flags: r.u8()?,
        })
    }
}
