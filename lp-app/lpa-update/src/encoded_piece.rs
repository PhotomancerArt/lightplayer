//! One piece in **encoding 1**: exactly a `.z` file and its chunk-length
//! index, as `ota-manifest.json`'s `encodings[]` entry `id: 1` lists them
//! (the firmware-distribution plan's files; `lpc_update::dictionary_rule`
//! says what the encoding means).
//!
//! - `stream` is the chunks' raw-deflate streams back to back, no header;
//! - `chunks[i]` is chunk `i`'s compressed length, **`0` = no compressed
//!   form, send raw** (it then adds no bytes to `stream`);
//! - so `chunks.len()` = the piece's chunk count, and the sum of `chunks` =
//!   `stream.len()`.

use alloc::vec::Vec;

use lpc_update::code_table::CHUNK;

/// The `.z` stream of one piece and its index.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EncodedPiece {
    pub stream: Vec<u8>,
    pub chunks: Vec<u32>,
}

/// Why an [`EncodedPiece`] cannot describe a piece.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodedPieceError {
    /// `chunks.len()` is not the piece's chunk count.
    ChunkCount { have: usize, need: usize },
    /// The index does not add up to the stream's length.
    StreamLength { indexed: u64, stream: usize },
}

impl EncodedPiece {
    /// Whether this is the encoding of a `piece_len`-byte piece, by its
    /// shape (the bytes are proven by the packer's `prove_piece`).
    pub fn check(&self, piece_len: usize) -> Result<(), EncodedPieceError> {
        let need = piece_len.div_ceil(CHUNK as usize);
        if self.chunks.len() != need {
            return Err(EncodedPieceError::ChunkCount {
                have: self.chunks.len(),
                need,
            });
        }
        let indexed: u64 = self.chunks.iter().map(|&c| u64::from(c)).sum();
        if indexed != self.stream.len() as u64 {
            return Err(EncodedPieceError::StreamLength {
                indexed,
                stream: self.stream.len(),
            });
        }
        Ok(())
    }

    /// Where each chunk's stream starts in `stream`.
    #[must_use]
    pub fn offsets(&self) -> Vec<u32> {
        let mut at = 0u32;
        self.chunks
            .iter()
            .map(|&len| {
                let here = at;
                at = at.saturating_add(len);
                here
            })
            .collect()
    }

    /// Compressed bytes over raw bytes, for the report (`< 1` is smaller).
    #[must_use]
    pub fn ratio(&self, piece_len: usize) -> f64 {
        if piece_len == 0 {
            return 1.0;
        }
        let raw_tail: u64 = self
            .chunks
            .iter()
            .enumerate()
            .filter(|(_, len)| **len == 0)
            .map(|(i, _)| (piece_len - i * CHUNK as usize).min(CHUNK as usize) as u64)
            .sum();
        (self.stream.len() as u64 + raw_tail) as f64 / piece_len as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn the_index_must_cover_the_piece_and_add_up() {
        let e = EncodedPiece {
            stream: vec![0; 30],
            chunks: vec![10, 0, 20],
        };
        assert_eq!(e.check(2 * 4096 + 1), Ok(()));
        assert_eq!(
            e.check(4096),
            Err(EncodedPieceError::ChunkCount { have: 3, need: 1 })
        );
        let bad = EncodedPiece {
            stream: vec![0; 29],
            chunks: vec![10, 0, 20],
        };
        assert!(matches!(
            bad.check(3 * 4096),
            Err(EncodedPieceError::StreamLength { .. })
        ));
        assert_eq!(e.offsets(), [0, 10, 10]);
    }

    #[test]
    fn the_ratio_counts_raw_chunks_at_their_size() {
        let e = EncodedPiece {
            stream: vec![0; 2048],
            chunks: vec![2048, 0],
        };
        assert!((e.ratio(8192) - (2048.0 + 4096.0) / 8192.0).abs() < 1e-9);
    }
}
