//! The update's dictionary window (spike): a compressed chunk (`Z`) may
//! refer back into the 32 KiB of the image just before it, which the board
//! has already written. The window keeps those bytes in RAM as the transfer
//! moves forward, and re-reads them from flash when it cannot follow (a
//! resume after a reset, or a chunk out of step).
//!
//! **The dictionary of a chunk at `off`** (both ends must agree — this is
//! the host's rule too, in `scripts/fw-split/compress.py`):
//! - core: the image's bytes `[max(0, off − 32 KiB), off)`;
//! - engine, `off ≥ 4 KiB`: `[max(4 KiB, off − 32 KiB), off)` — the header
//!   sector is written last, so it is never part of anyone's dictionary;
//! - engine, `off = 0` (the header): none.

use alloc::boxed::Box;
use alloc::vec;

use super::inflate::{self, inflate};
use super::split_flash::{SECTOR, SplitFlash};

const DICT: usize = 32 * 1024;

pub struct UpdateWindow {
    /// `buf[..len]` is the dictionary for the chunk at `at`.
    buf: Box<[u8]>,
    len: usize,
    at: Option<(u8, u32)>,
    /// Microseconds spent decoding, and how many chunks, for the log.
    pub decode_us: u64,
    pub decoded: u32,
}

fn lowest(kind: u8, off: u32) -> Option<u32> {
    match (kind, off) {
        (b'E', 0) => None,
        (b'E', _) => Some(SECTOR),
        _ => Some(0),
    }
}

impl UpdateWindow {
    pub fn new() -> Self {
        Self {
            buf: vec![0u8; DICT + SECTOR as usize].into_boxed_slice(),
            len: 0,
            at: None,
            decode_us: 0,
            decoded: 0,
        }
    }

    /// Decode `z` (the chunk of `kind` at `off`, image written at `dest`) and
    /// return its bytes. `None`: it did not decode to `expect` bytes.
    pub fn decode(
        &mut self,
        kind: u8,
        off: u32,
        dest: u32,
        expect: usize,
        z: &[u8],
        flash: &mut SplitFlash,
    ) -> Option<&[u8]> {
        if self.at != Some((kind, off)) && !self.load(kind, off, dest, flash) {
            return None;
        }
        let start = self.len;
        let t0 = embassy_time::Instant::now();
        let got = inflate(z, &mut self.buf[..start + expect], start);
        self.decode_us += t0.elapsed().as_micros();
        self.decoded += 1;
        match got {
            Ok(n) if n == expect => Some(&self.buf[start..start + n]),
            Ok(n) => {
                log::warn!("[OTA] chunk {off:#x} decoded to {n} B, not {expect}");
                None
            }
            Err(e) => {
                let e: inflate::Error = e;
                log::warn!("[OTA] chunk {off:#x} did not decode: {e:?}");
                None
            }
        }
    }

    /// The chunk at `off` (`n` bytes) was written: slide the window past it.
    /// After [`Self::decode`] its bytes are already in place; `data` is for a
    /// chunk that came uncompressed (`D`).
    pub fn written(&mut self, kind: u8, off: u32, n: usize, data: Option<&[u8]>) {
        if self.at != Some((kind, off)) {
            self.at = None; // out of step: the next `Z` re-reads from flash
            return;
        }
        if let Some(d) = data {
            self.buf[self.len..self.len + n].copy_from_slice(d);
        }
        let total = self.len + n;
        let keep = total.min(DICT);
        self.buf.copy_within(total - keep..total, 0);
        self.len = keep;
        let next = off + n as u32;
        // The engine's header sector (0) is never anyone's dictionary.
        self.at = Some((kind, next));
        if kind == b'E' && off == 0 {
            self.at = None;
        }
    }

    fn load(&mut self, kind: u8, off: u32, dest: u32, flash: &mut SplitFlash) -> bool {
        let Some(low) = lowest(kind, off) else {
            self.len = 0;
            self.at = Some((kind, off));
            return true;
        };
        let from = off.saturating_sub(DICT as u32).max(low);
        let n = (off - from) as usize;
        if n > 0 && !flash.read(dest + from, &mut self.buf[..n]) {
            return false;
        }
        self.len = n;
        self.at = Some((kind, off));
        true
    }
}
