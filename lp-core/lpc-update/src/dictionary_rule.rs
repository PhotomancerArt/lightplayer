//! **Encoding 1** — what `id: 1` means in `ota-manifest.json`'s `encodings[]`
//! and what request flag bit 0 asks for — is this rule. Forever.
//!
//! A piece is cut into chunks of [`CHUNK`] raw bytes (the last one short).
//! Each chunk is an **independent raw-deflate stream** (RFC 1951, no zlib or
//! gzip wrapper), compressed with a **preset dictionary**: the piece's own
//! bytes just before the chunk, at most a 32 KiB window, as named by
//! [`dictionary`]:
//!
//! - **core**, chunk at `off`: `[off − 32 KiB, off)`, clamped at 0;
//! - **engine**, chunk at `off ≥ 4096`: `[max(4096, off − 32 KiB), off)`;
//! - **engine**, chunk at `off = 0` (the header sector): **none**.
//!
//! The engine's header sector is written last, so it is never anyone's
//! dictionary, and its own chunk has none. A board decodes a chunk with
//! `lp_deflate::inflate`, the dictionary bytes read back from its own flash
//! (they are the piece's bytes, already written: chunks go in order).
//!
//! The files that carry encoding 1 — one `.z` stream per piece, the chunks'
//! streams back to back with no header, and a chunk-length index in
//! `ota-manifest.json` where `0` means "no compressed form, send raw" — are
//! the firmware-distribution plan's (its ADR 3). There is no container format
//! in this crate. The one packer is `lpa-update`'s `pack` feature.

use core::ops::Range;

use crate::code_table::CHUNK;
use crate::piece_kind::PieceKind;

/// The largest dictionary a chunk is compressed against.
pub const WINDOW: u32 = 32 * 1024;

/// The dictionary of the chunk at byte `off` of a `kind` piece, as a byte
/// range of that piece, or `None` when it has none. An empty range never
/// comes back as `Some`.
#[must_use]
pub fn dictionary(kind: PieceKind, off: u32) -> Option<Range<u32>> {
    let floor = match kind {
        PieceKind::Core => 0,
        PieceKind::Engine => {
            if off < CHUNK {
                return None;
            }
            CHUNK
        }
    };
    let start = off.saturating_sub(WINDOW).max(floor);
    (start < off).then_some(start..off)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_core_reaches_back_a_window_clamped_at_zero() {
        assert_eq!(dictionary(PieceKind::Core, 0), None);
        assert_eq!(dictionary(PieceKind::Core, 4096), Some(0..4096));
        assert_eq!(dictionary(PieceKind::Core, 32 * 1024), Some(0..32 * 1024));
        assert_eq!(
            dictionary(PieceKind::Core, 40 * 1024),
            Some(8 * 1024..40 * 1024)
        );
    }

    #[test]
    fn the_engine_header_is_never_a_dictionary_and_has_none() {
        assert_eq!(dictionary(PieceKind::Engine, 0), None);
        assert_eq!(
            dictionary(PieceKind::Engine, 4096),
            None,
            "chunk 1's would-be dictionary is the header: empty"
        );
        assert_eq!(dictionary(PieceKind::Engine, 8192), Some(4096..8192));
        // Chunk 9 (36 KiB) would reach back to 4 KiB exactly; chunk 8 is
        // clamped at 4 KiB, not 0.
        assert_eq!(
            dictionary(PieceKind::Engine, 9 * 4096),
            Some(4096..9 * 4096)
        );
        assert_eq!(
            dictionary(PieceKind::Engine, 8 * 4096),
            Some(4096..8 * 4096)
        );
        assert_eq!(
            dictionary(PieceKind::Engine, 10 * 4096),
            Some(2 * 4096..10 * 4096)
        );
    }
}
