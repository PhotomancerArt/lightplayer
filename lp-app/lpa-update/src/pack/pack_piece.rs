//! **The one packer of encoding 1** (feature `pack`, std). The firmware
//! distribution's `lp-cli firmware package` calls it to write the `.z`
//! files and fill `ota-manifest.json`'s `encodings[]` entry `id: 1`; its
//! `release-check` calls [`prove_piece`]. Nothing else compresses firmware.
//!
//! For each 4 KiB chunk of a piece (the last one short):
//!
//! 1. a **fresh** raw-deflate compressor at level 9
//!    (`flate2::Compress::new(Compression::best(), false)`, the `zlib-rs`
//!    backend: a dependency, Zlib licence, never copied source);
//! 2. `set_dictionary` with the piece's bytes the dictionary rule names
//!    (`lpc_update::dictionary_rule`), when that range is not empty;
//! 3. one `compress_vec(…, FlushCompress::Finish)`;
//! 4. **decoded back with `lp_deflate::inflate`** — the board's own decoder
//!    — over the same dictionary bytes, which must give exactly the chunk.
//!    A mismatch is a bug in the packer, and panics.
//!
//! A chunk whose compressed form is not smaller than raw gets index `0` (no
//! compressed form: send raw) and adds no bytes to the stream. The same
//! piece always gives the same bytes, so a release's manifest is a function
//! of its bytes.

use std::vec::Vec;

use flate2::{Compress, Compression, FlushCompress, Status};
use lpc_update::PieceKind;
use lpc_update::code_table::CHUNK;
use lpc_update::dictionary_rule::dictionary;

use crate::encoded_piece::EncodedPiece;

/// Encoding 1 of `piece` (a `kind` piece): its `.z` stream and index.
///
/// # Panics
///
/// If a chunk the compressor produced does not decode back to the exact
/// chunk with `lp_deflate`: the packer must never emit a chunk the board
/// cannot reproduce.
#[must_use]
pub fn pack_piece(kind: PieceKind, piece: &[u8]) -> EncodedPiece {
    let mut out = EncodedPiece::default();
    for (idx, chunk) in piece.chunks(CHUNK as usize).enumerate() {
        let off = idx as u32 * CHUNK;
        let dict = dictionary_bytes(kind, piece, off);
        match compress_chunk(dict, chunk) {
            Some(z) => {
                assert!(
                    decodes_to(&z, dict, chunk),
                    "the packer made a chunk lp-deflate does not decode back ({kind:?} chunk {idx})"
                );
                out.chunks.push(z.len() as u32);
                out.stream.extend_from_slice(&z);
            }
            None => out.chunks.push(0),
        }
    }
    out
}

/// Chunk `off`'s dictionary bytes, by the dictionary rule.
pub(crate) fn dictionary_bytes(kind: PieceKind, piece: &[u8], off: u32) -> &[u8] {
    dictionary(kind, off).map_or(&[], |r| &piece[r.start as usize..r.end as usize])
}

/// One chunk's raw-deflate stream against `dict`, or `None` when it would
/// not be smaller than the chunk.
fn compress_chunk(dict: &[u8], chunk: &[u8]) -> Option<Vec<u8>> {
    let mut c = Compress::new(Compression::best(), false);
    if !dict.is_empty() {
        c.set_dictionary(dict)
            .expect("a raw deflate stream takes a dictionary");
    }
    // Room for one byte less than raw: anything that does not fit is not
    // worth sending.
    let mut z = Vec::with_capacity(chunk.len().saturating_sub(1));
    match c.compress_vec(chunk, &mut z, FlushCompress::Finish) {
        Ok(Status::StreamEnd) if z.len() < chunk.len() => Some(z),
        _ => None,
    }
}

/// Whether `z` decodes, with `dict` as the preset dictionary, to exactly
/// `chunk` — through the board's decoder.
pub(crate) fn decodes_to(z: &[u8], dict: &[u8], chunk: &[u8]) -> bool {
    let mut buf = Vec::with_capacity(dict.len() + chunk.len());
    buf.extend_from_slice(dict);
    buf.resize(dict.len() + chunk.len(), 0);
    lp_deflate::inflate(z, &mut buf, dict.len()) == Ok(chunk.len()) && &buf[dict.len()..] == chunk
}

/// Why a piece and its encoding do not agree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProveError {
    /// The index does not describe the piece (count or stream length).
    Shape,
    /// Chunk `idx` does not decode back to the piece's bytes.
    Chunk { idx: u32 },
}

/// The packer's round trip from the files alone: every indexed chunk of
/// `encoded` decodes, against the dictionary rule's bytes of `piece`, to
/// exactly `piece`'s chunk. Names the first chunk that fails. The firmware
/// distribution's `release-check` runs it.
pub fn prove_piece(
    kind: PieceKind,
    piece: &[u8],
    encoded: &EncodedPiece,
) -> Result<(), ProveError> {
    encoded.check(piece.len()).map_err(|_| ProveError::Shape)?;
    let offsets = encoded.offsets();
    for (idx, chunk) in piece.chunks(CHUNK as usize).enumerate() {
        let len = encoded.chunks[idx] as usize;
        if len == 0 {
            continue;
        }
        let start = offsets[idx] as usize;
        let z = &encoded.stream[start..start + len];
        let off = idx as u32 * CHUNK;
        if !decodes_to(z, dictionary_bytes(kind, piece, off), chunk) {
            return Err(ProveError::Chunk { idx: idx as u32 });
        }
    }
    Ok(())
}
