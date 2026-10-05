//! A piece's chunks: their lengths and the order they go in.

use crate::code_table::CHUNK;
use crate::piece_kind::PieceKind;
use crate::transfer_record::{TransferRecord, chunk_count};

/// The length of chunk `idx` of a `len`-byte piece (the last one short).
#[must_use]
pub fn chunk_len(len: u32, idx: u32) -> u32 {
    len.saturating_sub(idx * CHUNK).min(CHUNK)
}

/// The chunk after `idx` in transfer order (the engine's header last), or
/// `None` when `idx` is the last.
#[must_use]
pub fn next_chunk(kind: PieceKind, len: u32, idx: u32) -> Option<u32> {
    let mut order = TransferRecord::order(kind, chunk_count(len));
    order.find(|&c| c == idx)?;
    order.next()
}

/// The first chunk in transfer order.
#[must_use]
pub fn first_chunk(kind: PieceKind, len: u32) -> Option<u32> {
    TransferRecord::order(kind, chunk_count(len)).next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_and_order() {
        assert_eq!(chunk_len(10_000, 0), 4096);
        assert_eq!(chunk_len(10_000, 2), 10_000 - 8192);
        assert_eq!(chunk_len(10_000, 3), 0);
        assert_eq!(first_chunk(PieceKind::Core, 10_000), Some(0));
        assert_eq!(next_chunk(PieceKind::Core, 10_000, 1), Some(2));
        assert_eq!(next_chunk(PieceKind::Core, 10_000, 2), None);
        assert_eq!(first_chunk(PieceKind::Engine, 10_000), Some(1));
        assert_eq!(next_chunk(PieceKind::Engine, 10_000, 2), Some(0));
        assert_eq!(next_chunk(PieceKind::Engine, 10_000, 0), None);
        assert_eq!(first_chunk(PieceKind::Engine, 100), Some(0));
    }
}
