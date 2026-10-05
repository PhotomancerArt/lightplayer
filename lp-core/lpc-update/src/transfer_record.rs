//! The **transfer-progress record**, v1 (DM12): what lets a transfer resume
//! after a reset, from any link, instead of starting its piece over.
//!
//! ```text
//!  0  magic     "LPUP"
//!  4  version   u16 = 1
//!  6  kind      u8  ('C' | 'E')
//!  7  stage     u8  (0 = pending: the running engine accepted it, nothing written;
//!                    1 = writing: core-only started it)
//!  8  build     u32 (the build hash of the build id, crate::build_id — the boot record's rule)
//! 12  dest      u32 (flash address of the piece's first byte)
//! 16  len       u32
//! 20  sha256    [32]
//! 52  crc       u32 (lp_crc32 of bytes 0..52)
//! 56  marks     ceil(len / 4096) bits, LSB first; a bit programmed 0 = that chunk
//!               was written and read back
//! ```
//!
//! It lives in its own sector, `factory + 0x5000` in layout 1
//! ([`crate::code_table::LAYOUT_1_PROGRESS_OFFSET`]). **Only that location is
//! forever**: a later core may change the shape, because of the
//! foreign-record rule below.
//!
//! # Writes
//!
//! The header (0..56) is written once, after the sector is erased, and never
//! changes: `stage` says who started the transfer and is inside the CRC. The
//! marks are programmed in place, 1 → 0 (NOR, no erase), outside the CRC,
//! one per chunk **after** that chunk was written and read back. The record
//! is erased when the piece commits.
//!
//! # Reading
//!
//! - A bad magic or CRC is **no record**.
//! - A torn mark reads as either value, and both are safe: a mark is only
//!   programmed after its chunk was read back, so "written" is true, and
//!   "not written" means the chunk is written again.
//! - [`TransferRecord::resume_at`] is the first chunk, in transfer order,
//!   whose mark is unset.
//!
//! # The foreign-record rule (`one-way-doors.md` §10) — part of v1
//!
//! A record is **foreign** when its version is not 1, or when it is neither
//! this core's engine transfer (kind `E`, `build` = this core's build hash,
//! `dest` = this core's engine start) nor its pending core transfer (kind
//! `C` with `dest` = where this core would place a core of that `len`, so
//! never over the running core, and not the build this board refused). A
//! foreign record is **ignored, then erased** when the next transfer starts.
//! This is what lets an old core meet a record a newer, failed core wrote
//! (after a rollback), and what keeps the shape free to change.

use crate::code_table::CHUNK;
use crate::piece_kind::PieceKind;

/// `"LPUP"`.
pub const RECORD_MAGIC: [u8; 4] = *b"LPUP";
/// This format.
pub const RECORD_VERSION: u16 = 1;
/// Bytes before the marks.
pub const RECORD_HEADER_LEN: usize = 56;
/// The CRC covers bytes `0..RECORD_CRC_OFFSET`.
pub const RECORD_CRC_OFFSET: usize = 52;

/// Who started a transfer (byte 7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordStage {
    /// 0: the running engine accepted the offer and reset into core-only;
    /// nothing of the piece is written (DM13).
    Pending,
    /// 1: core-only started it.
    Writing,
}

impl RecordStage {
    const fn byte(self) -> u8 {
        match self {
            Self::Pending => 0,
            Self::Writing => 1,
        }
    }
}

/// A v1 record's header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferRecord {
    pub kind: PieceKind,
    pub stage: RecordStage,
    pub build: u32,
    pub dest: u32,
    pub len: u32,
    pub sha256: [u8; 32],
}

/// What a progress sector holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordRead {
    /// Erased, or bytes that are not a record (bad magic or CRC).
    Nothing,
    /// A record of another version: foreign.
    OtherVersion(u16),
    /// A v1 record and its marks.
    V1(TransferRecord, MarkSet),
}

/// Whether a v1 record is this core's ([`TransferRecord::classify`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordClass {
    Own,
    Foreign,
}

/// What [`TransferRecord::classify`] needs to know about the running core.
#[derive(Clone, Copy, Debug)]
pub struct OwnFacts {
    /// This core's build hash.
    pub build_hash: u32,
    /// Where this core's engine starts.
    pub engine_start: u32,
    /// The build this board refused after a failed trial, if any.
    pub refused_build: Option<u32>,
}

/// The marks of one record: which chunks are written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkSet {
    /// One bit per chunk, `true` = written and read back.
    written: alloc::vec::Vec<bool>,
}

impl MarkSet {
    /// No chunk written.
    #[must_use]
    pub fn none(chunks: usize) -> Self {
        Self {
            written: alloc::vec![false; chunks],
        }
    }

    #[must_use]
    pub fn is_written(&self, chunk: u32) -> bool {
        self.written.get(chunk as usize).copied().unwrap_or(false)
    }

    pub fn set(&mut self, chunk: u32) {
        if let Some(w) = self.written.get_mut(chunk as usize) {
            *w = true;
        }
    }

    #[must_use]
    pub fn chunks(&self) -> u32 {
        self.written.len() as u32
    }

    /// How many chunks are written.
    #[must_use]
    pub fn count(&self) -> u32 {
        self.written.iter().filter(|w| **w).count() as u32
    }
}

impl TransferRecord {
    /// Chunks in the piece.
    #[must_use]
    pub const fn chunks(&self) -> u32 {
        chunk_count(self.len)
    }

    /// Bytes the marks take.
    #[must_use]
    pub const fn marks_len(&self) -> usize {
        self.chunks().div_ceil(8) as usize
    }

    /// The header, 56 bytes, CRC filled in. The marks after it are left to
    /// the erased state (`0xFF`).
    #[must_use]
    pub fn encode_header(&self) -> [u8; RECORD_HEADER_LEN] {
        let mut b = [0xFFu8; RECORD_HEADER_LEN];
        b[0..4].copy_from_slice(&RECORD_MAGIC);
        b[4..6].copy_from_slice(&RECORD_VERSION.to_le_bytes());
        b[6] = self.kind.byte();
        b[7] = self.stage.byte();
        b[8..12].copy_from_slice(&self.build.to_le_bytes());
        b[12..16].copy_from_slice(&self.dest.to_le_bytes());
        b[16..20].copy_from_slice(&self.len.to_le_bytes());
        b[20..52].copy_from_slice(&self.sha256);
        let crc = lp_crc32::crc32(&b[0..RECORD_CRC_OFFSET]);
        b[52..56].copy_from_slice(&crc.to_le_bytes());
        b
    }

    /// Read a progress sector (or at least its header and marks).
    #[must_use]
    pub fn read(sector: &[u8]) -> RecordRead {
        if sector.len() < 6 || sector[0..4] != RECORD_MAGIC {
            return RecordRead::Nothing;
        }
        let version = u16::from_le_bytes([sector[4], sector[5]]);
        if version != RECORD_VERSION {
            return RecordRead::OtherVersion(version);
        }
        let Some(header) = sector.get(..RECORD_HEADER_LEN) else {
            return RecordRead::Nothing;
        };
        let word = |at: usize| {
            u32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]])
        };
        if word(52) != lp_crc32::crc32(&header[..RECORD_CRC_OFFSET]) {
            return RecordRead::Nothing;
        }
        let (Some(kind), Some(stage)) = (
            PieceKind::from_byte(header[6]),
            match header[7] {
                0 => Some(RecordStage::Pending),
                1 => Some(RecordStage::Writing),
                _ => None,
            },
        ) else {
            return RecordRead::Nothing;
        };
        let mut sha256 = [0u8; 32];
        sha256.copy_from_slice(&header[20..52]);
        let record = Self {
            kind,
            stage,
            build: word(8),
            dest: word(12),
            len: word(16),
            sha256,
        };
        let mut marks = MarkSet::none(record.chunks() as usize);
        for chunk in 0..record.chunks() {
            let (byte, bit) = mark_position(chunk);
            if let Some(b) = sector.get(byte)
                && b & bit == 0
            {
                marks.set(chunk);
            }
        }
        RecordRead::V1(record, marks)
    }

    /// The foreign-record rule (see the module docs). `core_dest(len)` is
    /// where this core would place a core of `len` bytes.
    #[must_use]
    pub fn classify(&self, own: &OwnFacts, core_dest: impl Fn(u32) -> Option<u32>) -> RecordClass {
        let own_record = match self.kind {
            PieceKind::Engine => self.build == own.build_hash && self.dest == own.engine_start,
            PieceKind::Core => {
                Some(self.build) != own.refused_build && core_dest(self.len) == Some(self.dest)
            }
        };
        if own_record {
            RecordClass::Own
        } else {
            RecordClass::Foreign
        }
    }

    /// The order chunks go in: the core front to back; the engine's sectors
    /// 1..n, then sector 0 (its header) last.
    #[must_use]
    pub fn order(kind: PieceKind, chunks: u32) -> impl Iterator<Item = u32> {
        let (first, header_last) = match kind {
            PieceKind::Core => (0, 0),
            PieceKind::Engine => (1, chunks.min(1)),
        };
        (first..chunks).chain(0..header_last)
    }

    /// The first chunk, in transfer order, not yet written; `None` when every
    /// chunk is.
    #[must_use]
    pub fn resume_at(&self, marks: &MarkSet) -> Option<u32> {
        Self::order(self.kind, self.chunks()).find(|&c| !marks.is_written(c))
    }
}

/// Chunks in a piece of `len` bytes.
#[must_use]
pub const fn chunk_count(len: u32) -> u32 {
    len.div_ceil(CHUNK)
}

/// Where chunk `chunk`'s mark is: `(byte offset in the sector, bit mask)`.
/// Programming `!mask` into that byte (NOR: 1 → 0) sets it.
#[must_use]
pub const fn mark_position(chunk: u32) -> (usize, u8) {
    (RECORD_HEADER_LEN + (chunk / 8) as usize, 1 << (chunk % 8))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn record(kind: PieceKind) -> TransferRecord {
        TransferRecord {
            kind,
            stage: RecordStage::Writing,
            build: 0x1234_5678,
            dest: 0x0038_0000,
            len: 10 * 4096 + 100,
            sha256: [7; 32],
        }
    }

    fn sector(rec: &TransferRecord) -> Vec<u8> {
        let mut s = alloc::vec![0xFFu8; 4096];
        s[..RECORD_HEADER_LEN].copy_from_slice(&rec.encode_header());
        s
    }

    fn program_mark(s: &mut [u8], chunk: u32) {
        let (byte, bit) = mark_position(chunk);
        s[byte] &= !bit;
    }

    #[test]
    fn a_fresh_record_reads_back_with_no_marks() {
        let rec = record(PieceKind::Core);
        let RecordRead::V1(back, marks) = TransferRecord::read(&sector(&rec)) else {
            panic!("not a record");
        };
        assert_eq!(back, rec);
        assert_eq!(marks.count(), 0);
        assert_eq!(rec.chunks(), 11);
        assert_eq!(rec.marks_len(), 2);
        assert_eq!(back.resume_at(&marks), Some(0));
    }

    #[test]
    fn a_header_truncated_or_damaged_at_any_byte_is_no_record() {
        let rec = record(PieceKind::Engine);
        let full = sector(&rec);
        for cut in 0..RECORD_HEADER_LEN {
            let mut torn = full.clone();
            for b in &mut torn[cut..RECORD_HEADER_LEN] {
                *b = 0xFF;
            }
            assert!(
                !matches!(TransferRecord::read(&torn), RecordRead::V1(..)),
                "a header written only up to byte {cut} must not read as a record"
            );
            assert!(!matches!(
                TransferRecord::read(&full[..cut]),
                RecordRead::V1(..)
            ));
        }
        for at in 0..RECORD_CRC_OFFSET {
            if (4..6).contains(&at) {
                continue; // the version: OtherVersion, tested below
            }
            let mut bad = full.clone();
            bad[at] ^= 0x10;
            assert!(
                !matches!(TransferRecord::read(&bad), RecordRead::V1(..)),
                "a flipped byte {at} must fail the CRC or the magic"
            );
        }
    }

    #[test]
    fn each_mark_bit_reads_as_written_alone() {
        let rec = record(PieceKind::Core);
        for chunk in 0..rec.chunks() {
            let mut s = sector(&rec);
            program_mark(&mut s, chunk);
            let RecordRead::V1(_, marks) = TransferRecord::read(&s) else {
                panic!("marks are outside the CRC");
            };
            for c in 0..rec.chunks() {
                assert_eq!(marks.is_written(c), c == chunk);
            }
        }
    }

    #[test]
    fn resume_is_the_first_unmarked_chunk_in_transfer_order() {
        let core = record(PieceKind::Core);
        let mut s = sector(&core);
        for c in 0..4 {
            program_mark(&mut s, c);
        }
        let RecordRead::V1(_, marks) = TransferRecord::read(&s) else {
            panic!()
        };
        assert_eq!(core.resume_at(&marks), Some(4));

        let engine = record(PieceKind::Engine);
        let order: Vec<u32> = TransferRecord::order(PieceKind::Engine, engine.chunks()).collect();
        assert_eq!(order.first(), Some(&1));
        assert_eq!(order.last(), Some(&0), "the header goes last");
        let mut s = sector(&engine);
        for c in 1..engine.chunks() {
            program_mark(&mut s, c);
        }
        let RecordRead::V1(_, marks) = TransferRecord::read(&s) else {
            panic!()
        };
        assert_eq!(engine.resume_at(&marks), Some(0), "only the header is left");
        program_mark(&mut s, 0);
        let RecordRead::V1(_, marks) = TransferRecord::read(&s) else {
            panic!()
        };
        assert_eq!(engine.resume_at(&marks), None);
        // A one-chunk engine is its header alone.
        assert_eq!(
            TransferRecord::order(PieceKind::Engine, 1).collect::<Vec<_>>(),
            [0]
        );
    }

    #[test]
    fn foreign_records_classify_as_foreign() {
        let own = OwnFacts {
            build_hash: 0xAAAA_0001,
            engine_start: 0x0020_0000,
            refused_build: Some(0xBAD0_0001),
        };
        let core_dest = |len: u32| (len < 0x10_0000).then_some(0x0030_0000);

        // Version 2: foreign whatever it says.
        let mut v2 = sector(&record(PieceKind::Core));
        v2[4] = 2;
        assert_eq!(TransferRecord::read(&v2), RecordRead::OtherVersion(2));

        // This core's own engine transfer.
        let mut e = record(PieceKind::Engine);
        e.build = own.build_hash;
        e.dest = own.engine_start;
        assert_eq!(e.classify(&own, core_dest), RecordClass::Own);
        // Another build's engine transfer.
        e.build = 0xAAAA_0002;
        assert_eq!(e.classify(&own, core_dest), RecordClass::Foreign);

        // A pending core transfer where this core would put it.
        let mut c = record(PieceKind::Core);
        c.dest = 0x0030_0000;
        assert_eq!(c.classify(&own, core_dest), RecordClass::Own);
        // A core transfer over the running core (anywhere else).
        c.dest = 0x0001_8000;
        assert_eq!(c.classify(&own, core_dest), RecordClass::Foreign);
        // The build this board refused.
        c.dest = 0x0030_0000;
        c.build = 0xBAD0_0001;
        assert_eq!(c.classify(&own, core_dest), RecordClass::Foreign);
    }

    #[test]
    fn erased_flash_is_no_record() {
        assert_eq!(TransferRecord::read(&[0xFF; 4096]), RecordRead::Nothing);
        assert_eq!(TransferRecord::read(&[]), RecordRead::Nothing);
    }
}
