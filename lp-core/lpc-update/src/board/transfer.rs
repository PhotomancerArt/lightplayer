//! One piece in flight: its record, its marks, who owns it, and which chunk
//! the session waits for.

use alloc::vec::Vec;

use crate::piece_kind::PieceKind;
use crate::transfer_record::{MarkSet, TransferRecord};

use super::board_link::LinkId;
use super::transfer_plan::chunk_len;

/// A transfer the session runs, or has restored from the progress record
/// and waits to resume.
pub(crate) struct Transfer {
    /// What the progress record says (kind, build hash, dest, len, SHA-256).
    pub record: TransferRecord,
    pub marks: MarkSet,
    /// The link that started or resumed it. `None` after a restore: the
    /// next matching offer, from any link, resumes it.
    pub owner: Option<LinkId>,
    /// The chunk requested and not yet written. `None` while nobody drives
    /// the transfer (restored, or stopped by a flash fault).
    pub waiting: Option<u32>,
    /// Ask for the waiting chunk raw: a `Z` for it did not decode.
    pub raw_next: bool,
    /// Read-back mismatches on the waiting chunk, to give the piece up
    /// rather than rewrite a sector that will not take.
    pub retries: u8,
    /// The engine's header sector, held in RAM until the piece hashes.
    pub sector0: Option<Vec<u8>>,
}

impl Transfer {
    pub(crate) fn new(record: TransferRecord, marks: MarkSet) -> Self {
        Self {
            record,
            marks,
            owner: None,
            waiting: None,
            raw_next: false,
            retries: 0,
            sector0: None,
        }
    }

    pub(crate) fn kind(&self) -> PieceKind {
        self.record.kind
    }

    /// Bytes written and read back.
    pub(crate) fn done_bytes(&self) -> u32 {
        (0..self.record.chunks())
            .filter(|&c| self.marks.is_written(c))
            .map(|c| chunk_len(self.record.len, c))
            .sum()
    }

    /// Whether `other` (the record an offer would write) is this transfer:
    /// the same piece of the same build, to the same place.
    pub(crate) fn matches(&self, other: &TransferRecord) -> bool {
        self.record.kind == other.kind
            && self.record.build == other.build
            && self.record.dest == other.dest
            && self.record.len == other.len
            && self.record.sha256 == other.sha256
    }
}
