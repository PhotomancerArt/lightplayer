//! The read-back backup (D2, DM17): before another version goes on, the
//! host reads the board's running engine back and keeps it, so the board
//! can always be healed to what it ran.
//!
//! - Sends `G E off piece`, keeping `ahead` outstanding. The piece is a whole
//!   chunk (4 KiB) unless the link asks for less: over Bluetooth a read-back
//!   answer must fit the radio link's own send ring
//!   ([`crate::serve::BLE_READ_BACK_PIECE`]), or it holds the board's one
//!   shared frame buffer while it crawls out and every other reply waits.
//! - Collects the board's `D`s in any order, ignoring duplicates.
//! - When every piece is in, checks SHA-256 against the manifest's
//!   `engineSha256`: a match is [`BackupStep::Done`] with the bytes, a
//!   mismatch [`BackupStep::Failed`].
//! - After a link drop, [`BackupSession::resume`] asks again for what is
//!   missing.
//! - A piece asked for and still unanswered after a while is asked for
//!   again ([`BackupSession::reask_stale`]): an answer can be lost with the
//!   link up (2026-10-07 desk run d2: a backup stuck at 36 % for five
//!   minutes while the pieces after the hole kept arriving), and nothing
//!   else would ever ask for it.
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
    /// Every piece is in and hashes to the manifest's `engineSha256`.
    Done(Vec<u8>),
    /// Every piece is in and the bytes do not hash to it.
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
    /// Bytes per `G` (the last piece is shorter).
    piece: u32,
    ahead: u8,
    bytes: Vec<u8>,
    have: Vec<bool>,
    /// When each piece asked for and not yet received was asked (`None`:
    /// not outstanding).
    asked: Vec<Option<u64>>,
    /// The caller's clock, for the asks it stamps ([`Self::set_now`]).
    now_ms: u64,
    finished: bool,
}

impl BackupSession {
    /// Back up an engine of `len` bytes that must hash to `expected`, a
    /// whole chunk per `G`.
    #[must_use]
    pub fn new(expected: [u8; 32], len: u32, ahead: u8) -> Self {
        Self::with_piece(expected, len, ahead, CHUNK)
    }

    /// [`Self::new`], `piece` bytes per `G` (1..=one chunk: the board answers
    /// at most one chunk per `G`).
    #[must_use]
    pub fn with_piece(expected: [u8; 32], len: u32, ahead: u8, piece: u32) -> Self {
        let piece = piece.clamp(1, CHUNK);
        let pieces = len.div_ceil(piece) as usize;
        Self {
            expected,
            len,
            piece,
            ahead: ahead.max(1),
            bytes: vec![0; len as usize],
            have: vec![false; pieces],
            asked: vec![None; pieces],
            now_ms: 0,
            finished: false,
        }
    }

    /// The caller's clock: what the next asks are stamped with.
    pub fn set_now(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }

    /// The first `G`s.
    pub fn start(&mut self) -> Vec<Vec<u8>> {
        self.top_up()
    }

    /// The link dropped and came back: ask again for every missing piece,
    /// from the lowest, up to `ahead`.
    pub fn resume(&mut self) -> Vec<Vec<u8>> {
        self.asked.fill(None);
        self.top_up()
    }

    /// Ask again for every piece asked at least `older_than_ms` ago and
    /// still not in (its `G` or its `D` was lost with the link up), lowest
    /// first, and top up.
    pub fn reask_stale(&mut self, older_than_ms: u64) -> Vec<Vec<u8>> {
        let now = self.now_ms;
        let mut stale = false;
        for asked in &mut self.asked {
            if asked.is_some_and(|at| now.saturating_sub(at) >= older_than_ms) {
                *asked = None;
                stale = true;
            }
        }
        if !stale {
            return Vec::new();
        }
        self.top_up()
    }

    /// Bytes received so far (contiguous from the start).
    #[must_use]
    pub fn contiguous(&self) -> u32 {
        let n = self.have.iter().take_while(|h| **h).count() as u32;
        (n * self.piece).min(self.len)
    }

    /// One board message; anything but a read-back `D` of the engine is
    /// ignored (`Send` of nothing).
    pub fn on_board(&mut self, bytes: &[u8]) -> BackupStep {
        let Ok(BoardMessage::Data(d)) = BoardMessage::decode(bytes) else {
            return BackupStep::Send(Vec::new());
        };
        if self.finished || d.kind != PieceKind::Engine || d.off % self.piece != 0 {
            return BackupStep::Send(Vec::new());
        }
        let idx = (d.off / self.piece) as usize;
        let want = (self.len - d.off.min(self.len)).min(self.piece) as usize;
        if idx >= self.have.len() || self.have[idx] || d.payload.len() != want {
            return BackupStep::Send(Vec::new());
        }
        let at = d.off as usize;
        self.bytes[at..at + want].copy_from_slice(d.payload);
        self.have[idx] = true;
        self.asked[idx] = None;
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
        let outstanding = self.asked.iter().filter(|a| a.is_some()).count();
        let mut room = usize::from(self.ahead).saturating_sub(outstanding);
        for idx in 0..self.have.len() {
            if room == 0 {
                break;
            }
            if self.have[idx] || self.asked[idx].is_some() {
                continue;
            }
            self.asked[idx] = Some(self.now_ms);
            room -= 1;
            let g = ReadBackRequest {
                kind: PieceKind::Engine,
                off: idx as u32 * self.piece,
                len: self.piece,
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

    fn send_of(step: BackupStep) -> Vec<Vec<u8>> {
        match step {
            BackupStep::Send(gs) => gs,
            other => panic!("{other:?}"),
        }
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
    fn pieces_smaller_than_a_chunk_ask_and_take_that_much() {
        let e = engine();
        let mut b = BackupSession::with_piece(engine_sha256(&e), e.len() as u32, 2, 1016);
        let first = b.start();
        assert_eq!(offs(&first), [0, 1016]);
        let lens: Vec<u32> = first
            .iter()
            .map(|g| match HostMessage::decode(g) {
                Ok(HostMessage::ReadBack(r)) => r.len,
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(lens, [1016, 1016]);
        let mut step = BackupStep::Send(Vec::new());
        for off in (0..e.len() as u32).step_by(1016) {
            let end = (off as usize + 1016).min(e.len());
            let d = encode_chunk(
                ChunkEncoding::Raw,
                PieceKind::Engine,
                off,
                &e[off as usize..end],
            );
            assert!(
                d.len() <= 1024,
                "a 1016 B piece's D fits a radio link's send ring"
            );
            step = b.on_board(&d);
        }
        assert_eq!(step, BackupStep::Done(e));
    }

    #[test]
    fn a_piece_whose_answer_never_came_is_asked_again_once_it_is_stale() {
        let e = engine();
        let mut b = BackupSession::new(engine_sha256(&e), e.len() as u32, 2);
        b.set_now(1_000);
        assert_eq!(offs(&b.start()), [0, 4096]);
        // Chunk 0's answer is lost; chunk 1's arrives and the top-up asks 2.
        b.set_now(1_500);
        assert_eq!(offs(&send_of(b.on_board(&data(&e, 1)))), [8192]);
        assert_eq!(b.contiguous(), 0);
        // Not stale yet: nothing asked again.
        b.set_now(10_999);
        assert!(b.reask_stale(10_000).is_empty());
        // Stale now: chunk 0 is asked again (chunk 2, asked later, is not).
        b.set_now(11_000);
        assert_eq!(offs(&b.reask_stale(10_000)), [0]);
        b.on_board(&data(&e, 0));
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
