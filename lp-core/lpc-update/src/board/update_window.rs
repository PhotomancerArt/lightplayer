//! Decoding `Z` (encoding 1, DM18): a window buffer of
//! `[dictionary | chunk]`, `32 KiB + 4 KiB`, allocated when the first `Z`
//! arrives and kept in step with what the transfer writes.
//!
//! The dictionary is the piece's own preceding bytes
//! ([`crate::dictionary_rule`]), already written: chunks go in order. When
//! the window is out of step — after a resume, a reset, or anything that
//! skipped it — it is re-read from flash. Inflate is never trusted for
//! integrity: the piece's SHA-256 is the check (a flipped stored block
//! decodes "fine").

use alloc::vec;
use alloc::vec::Vec;

use crate::code_table::CHUNK;
use crate::dictionary_rule::{WINDOW, dictionary};
use crate::piece_kind::PieceKind;

use super::update_target::{FlashFault, UpdateTarget};

/// Why a `Z` gave no chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowError {
    /// The stream did not decode to exactly the chunk's length: re-request
    /// it raw.
    Decode,
    /// Reading the dictionary from flash failed.
    Flash(FlashFault),
}

/// The window: piece bytes `[start, start + len)` of the piece at `dest`.
pub(crate) struct UpdateWindow {
    buf: Vec<u8>,
    piece: Option<(PieceKind, u32)>,
    start: u32,
    len: usize,
}

impl UpdateWindow {
    pub(crate) fn new() -> Self {
        Self {
            buf: vec![0u8; (WINDOW + CHUNK) as usize],
            piece: None,
            start: 0,
            len: 0,
        }
    }

    /// Decode chunk `off` (`chunk_len` bytes) of the `kind` piece at `dest`
    /// from `src`. On success the decoded bytes are returned and the window
    /// holds them, ready for the next chunk.
    pub(crate) fn decode<T: UpdateTarget>(
        &mut self,
        target: &mut T,
        kind: PieceKind,
        dest: u32,
        off: u32,
        chunk_len: usize,
        src: &[u8],
    ) -> Result<&[u8], WindowError> {
        let dict = dictionary(kind, off).unwrap_or(off..off);
        let dict_len = (dict.end - dict.start) as usize;
        let in_step = self.piece == Some((kind, dest))
            && self.start <= dict.start
            && self.start as usize + self.len == off as usize;
        if in_step {
            // Drop what is older than the dictionary.
            let drop = (dict.start - self.start) as usize;
            self.buf.copy_within(drop..self.len, 0);
        } else if dict_len > 0 {
            target
                .read(dest + dict.start, &mut self.buf[..dict_len])
                .map_err(WindowError::Flash)?;
        }
        self.piece = Some((kind, dest));
        self.start = dict.start;
        self.len = dict_len;
        let out = &mut self.buf[..dict_len + chunk_len];
        match lp_deflate::inflate(src, out, dict_len) {
            Ok(n) if n == chunk_len => {
                self.len = dict_len + chunk_len;
                Ok(&self.buf[dict_len..dict_len + chunk_len])
            }
            _ => {
                // The window still holds the dictionary, in step.
                Err(WindowError::Decode)
            }
        }
    }

    /// A chunk was written some other way (raw): keep the window in step if
    /// it was, so the next `Z` needs no flash read.
    pub(crate) fn note_written(&mut self, kind: PieceKind, dest: u32, off: u32, bytes: &[u8]) {
        let in_step =
            self.piece == Some((kind, dest)) && self.start as usize + self.len == off as usize;
        if !in_step {
            self.piece = None;
            return;
        }
        let cap = self.buf.len();
        if self.len + bytes.len() > cap {
            let drop = self.len + bytes.len() - cap;
            self.buf.copy_within(drop..self.len, 0);
            self.len -= drop;
            self.start += drop as u32;
        }
        self.buf[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
    }
}
