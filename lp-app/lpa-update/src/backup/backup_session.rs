//! The read-back backup (D2, DM17): before another version goes on, the
//! host reads the board's running engine back and keeps it, so the board
//! can always be healed to what it ran.
//!
//! - Sends `G E off 4096`, keeping `ahead` outstanding.
//! - Collects the board's `D`s in any order, ignoring duplicates.
//! - When every chunk is in, checks SHA-256 against the manifest's
//!   `engineSha256`: a match is [`BackupStep::Done`] with the bytes, a
//!   mismatch [`BackupStep::Failed`].
//! - After a link drop, [`BackupSession::resume`] asks again for what is
//!   missing.
//!
//! The engine's length is the manifest's `engineLen`, present whenever
//! read-back is served (the header is valid). **No host ever parses an
//! engine header** (doors #12), which keeps the header a per-build format.

use alloc::vec;
use alloc::vec::Vec;

use lpc_update::code_table::CHUNK;
use lpc_update::hash_rules::engine_sha256;
use lpc_update::{BoardMessage, HostMessage, PieceKind, ReadBackRequest};

/// What one board message did to the backup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackupStep {
    /// Send these `G`s (possibly none) and keep going.
    Send(Vec<Vec<u8>>),
    /// Every chunk is in and hashes to the manifest's `engineSha256`.
    Done(Vec<u8>),
    /// Every chunk is in and the bytes do not hash to it.
    Failed(BackupError),
}

/// Why a backup failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackupError {
    Mismatch,
}

/// One engine's read-back.
#[derive(Clone, Debug)]
pub struct BackupSession {
    expected: [u8; 32],
    len: u32,
    ahead: u8,
    bytes: Vec<u8>,
    have: Vec<bool>,
    /// Chunks asked for and not yet received.
    asked: Vec<bool>,
    finished: bool,
}

impl BackupSession {
    /// Back up an engine of `len` bytes that must hash to `expected`.
    #[must_use]
    pub fn new(expected: [u8; 32], len: u32, ahead: u8) -> Self {
        let chunks = len.div_ceil(CHUNK) as usize;
        Self {
            expected,
            len,
            ahead: ahead.max(1),
            bytes: vec![0; len as usize],
            have: vec![false; chunks],
            asked: vec![false; chunks],
            finished: false,
        }
    }

    /// The first `G`s.
    pub fn start(&mut self) -> Vec<Vec<u8>> {
        self.top_up()
    }

    /// The link dropped and came back: ask again for every missing chunk,
    /// from the lowest, up to `ahead`.
    pub fn resume(&mut self) -> Vec<Vec<u8>> {
        self.asked.fill(false);
        self.top_up()
    }

    /// Bytes received so far (contiguous from the start).
    #[must_use]
    pub fn contiguous(&self) -> u32 {
        let n = self.have.iter().take_while(|h| **h).count() as u32;
        (n * CHUNK).min(self.len)
    }

    /// One board message; anything but a read-back `D` of the engine is
    /// ignored (`Send` of nothing).
    pub fn on_board(&mut self, bytes: &[u8]) -> BackupStep {
        let Ok(BoardMessage::Data(d)) = BoardMessage::decode(bytes) else {
            return BackupStep::Send(Vec::new());
        };
        if self.finished || d.kind != PieceKind::Engine || d.off % CHUNK != 0 {
            return BackupStep::Send(Vec::new());
        }
        let idx = (d.off / CHUNK) as usize;
        let want = (self.len - d.off.min(self.len)).min(CHUNK) as usize;
        if idx >= self.have.len() || self.have[idx] || d.payload.len() != want {
            return BackupStep::Send(Vec::new());
        }
        let at = d.off as usize;
        self.bytes[at..at + want].copy_from_slice(d.payload);
        self.have[idx] = true;
        self.asked[idx] = false;
        if self.have.iter().all(|h| *h) {
            self.finished = true;
            return if engine_sha256(&self.bytes) == self.expected {
                BackupStep::Done(core::mem::take(&mut self.bytes))
            } else {
                BackupStep::Failed(BackupError::Mismatch)
            };
        }
        BackupStep::Send(self.top_up())
    }

    fn top_up(&mut self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let outstanding = self.asked.iter().filter(|a| **a).count();
        let mut room = usize::from(self.ahead).saturating_sub(outstanding);
        for idx in 0..self.have.len() {
            if room == 0 {
                break;
            }
            if self.have[idx] || self.asked[idx] {
                continue;
            }
            self.asked[idx] = true;
            room -= 1;
            let g = ReadBackRequest {
                kind: PieceKind::Engine,
                off: idx as u32 * CHUNK,
                len: CHUNK,
            };
            out.push(HostMessage::ReadBack(g).encode());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_update::{ChunkEncoding, encode_chunk};

    fn engine() -> Vec<u8> {
        (0..10_000u32).map(|i| (i * 13) as u8).collect()
    }

    fn data(e: &[u8], idx: u32) -> Vec<u8> {
        let start = (idx * CHUNK) as usize;
        let end = (start + CHUNK as usize).min(e.len());
        encode_chunk(
            ChunkEncoding::Raw,
            PieceKind::Engine,
            idx * CHUNK,
            &e[start..end],
        )
    }

    fn offs(gs: &[Vec<u8>]) -> Vec<u32> {
        gs.iter()
            .map(|g| match HostMessage::decode(g) {
                Ok(HostMessage::ReadBack(r)) => r.off,
                other => panic!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn in_order_with_one_outstanding() {
        let e = engine();
        let mut b = BackupSession::new(engine_sha256(&e), e.len() as u32, 1);
        assert_eq!(offs(&b.start()), [0]);
        assert_eq!(
            b.on_board(&data(&e, 0)),
            BackupStep::Send(vec![
                HostMessage::ReadBack(ReadBackRequest {
                    kind: PieceKind::Engine,
                    off: 4096,
                    len: 4096
                })
                .encode()
            ])
        );
        b.on_board(&data(&e, 1));
        assert_eq!(b.on_board(&data(&e, 2)), BackupStep::Done(e));
    }

    #[test]
    fn out_of_order_and_duplicates_with_several_outstanding() {
        let e = engine();
        let mut b = BackupSession::new(engine_sha256(&e), e.len() as u32, 4);
        assert_eq!(offs(&b.start()), [0, 4096, 8192]);
        assert_eq!(b.on_board(&data(&e, 2)), BackupStep::Send(vec![]));
        assert_eq!(
            b.on_board(&data(&e, 2)),
            BackupStep::Send(vec![]),
            "a duplicate"
        );
        assert_eq!(b.on_board(&data(&e, 0)), BackupStep::Send(vec![]));
        assert_eq!(b.on_board(&data(&e, 1)), BackupStep::Done(e));
    }

    #[test]
    fn a_resume_after_a_drop_asks_only_for_what_is_missing() {
        let e = engine();
        let mut b = BackupSession::new(engine_sha256(&e), e.len() as u32, 2);
        b.start();
        b.on_board(&data(&e, 0));
        assert_eq!(b.contiguous(), 4096);
        // The link dropped with chunks 1 and 2 in flight.
        assert_eq!(offs(&b.resume()), [4096, 8192]);
        b.on_board(&data(&e, 1));
        assert_eq!(b.on_board(&data(&e, 2)), BackupStep::Done(e));
    }

    #[test]
    fn bytes_that_do_not_hash_fail() {
        let e = engine();
        let mut b = BackupSession::new([0; 32], e.len() as u32, 4);
        b.start();
        b.on_board(&data(&e, 0));
        b.on_board(&data(&e, 1));
        assert_eq!(
            b.on_board(&data(&e, 2)),
            BackupStep::Failed(BackupError::Mismatch)
        );
    }
}
