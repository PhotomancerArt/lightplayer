//! `G`, host→board: read back part of a piece (DM17).
//!
//! ```text
//! 'G' kind:u8 off:u32 len:u32
//! ```
//!
//! v1 reads back the engine only (kind `E`): the board answers each `G` with
//! one board→host `D` of at most one chunk, from the engine extent, while the
//! engine header is valid. A host backs the running engine up with it before
//! putting another version on (D2).

use alloc::vec::Vec;

use crate::piece_kind::PieceKind;
use crate::wire_reader::WireReader;

/// The `G` message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadBackRequest {
    pub kind: PieceKind,
    pub off: u32,
    pub len: u32,
}

impl ReadBackRequest {
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(10);
        out.push(b'G');
        out.push(self.kind.byte());
        out.extend_from_slice(&self.off.to_le_bytes());
        out.extend_from_slice(&self.len.to_le_bytes());
        out
    }

    pub(crate) fn decode(r: &mut WireReader<'_>) -> Option<Self> {
        Some(Self {
            kind: PieceKind::from_byte(r.u8()?)?,
            off: r.u32()?,
            len: r.u32()?,
        })
    }
}
