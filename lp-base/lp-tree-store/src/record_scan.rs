//! Mount's first pass over one sector: every record header and payload,
//! CRC-checked (a torn program can leave a good header over a short payload
//! — README defect 1), stopping at the end or at the first record that does
//! not check; a record of an unknown kind is checked the same way and
//! skipped. Nothing is indexed here (that would cost RAM for every record
//! on flash, garbage included): the pass finds where each sector's trusted
//! records end and keeps the roots, two at most (I1 falls back one step and
//! no further). `mount_walk.rs` then indexes only the chosen root's closure.
//!
//! **Settling a head's tail.** A CRC that passed is a fact about one read,
//! not about the cells: a program torn inside a 4-byte word can leave its
//! one missing clear *weak* (CX1, 2026-10-14), and the record then reads
//! exactly as written on some reads and fails its CRC on others. Only the
//! last program in the newest sector of each kind can be torn (every
//! program before it was read back whole), so mount settles that one before
//! anything trusts it: when the sector's records end at erased space, its
//! last record is programmed again with the bytes this read checked, and
//! read back (it then reads so on every read: program only clears, and the
//! bits it clears are the ones the record asked for); when they end at a
//! record that did not check, that record's header is programmed to zeros,
//! so it fails on every read; a sector holding no record (its last program
//! was its header's magic) is not resumed. Either way a later mount reads
//! what this one read
//! (`docs/defects/2026-10-14-tree-store-trusts-a-torn-record-that-reads-whole-once.md`).

use alloc::vec::Vec;

use crate::flash::Flash;
use crate::mount_verdict::FormatResidue;
use crate::object_id::ObjectId;
use crate::ram_index::RecordLoc;
use crate::record_header::{HeaderRead, RECORD_HEADER_LEN, RecordHeader};
use crate::record_kind::RecordKind;
use crate::record_log::RecordLog;
use crate::root_record::RootRecord;
use crate::sector_header::SECTOR_HEADER_LEN;
use crate::store_error::StoreError;

/// A root found by the scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RootCandidate {
    pub seq: u64,
    pub id: ObjectId,
    pub loc: RecordLoc,
}

/// The newest two roots seen, and the highest seq of any.
#[derive(Default)]
pub struct RootCandidates {
    pub best: [Option<RootCandidate>; 2],
    pub max_seq: u64,
}

impl RootCandidates {
    /// Offered in sector-sequence order: a second copy of a root (a GC
    /// copy) moves it to the later sector, the copy FORMAT.md says to keep.
    fn offer(&mut self, c: RootCandidate) {
        self.max_seq = self.max_seq.max(c.seq);
        if let Some(b) = self.best.iter_mut().flatten().find(|b| b.id == c.id) {
            b.loc = c.loc;
            return;
        }
        match self.best {
            [Some(a), _] if c.seq <= a.seq => {
                if self.best[1].is_none_or(|b| c.seq > b.seq) && c.id != a.id {
                    self.best[1] = Some(c);
                }
            }
            _ => {
                self.best[1] = self.best[0];
                self.best[0] = Some(c);
            }
        }
    }
}

/// How a sector ended.
#[derive(Clone, Copy, Debug)]
pub struct ScannedSector {
    /// Offset just past the last good record: the sector's trusted records
    /// are `[SECTOR_HEADER_LEN, end)`, closed or not.
    pub end: u32,
    /// A record failed to check: nothing after it is trusted.
    pub closed: bool,
}

/// Scan sector `s`, whose header was valid. Every CRC-good record is also
/// shown to `residue` (`mount_verdict.rs`: was anything here beyond what
/// `format` writes?). `settle`: `s` is the newest sector of its kind, and
/// its tail is settled (above); a tail that does not read back as settled
/// closes the sector.
pub fn scan_sector<F: Flash>(
    log: &mut RecordLog<F>,
    s: u32,
    roots: &mut RootCandidates,
    residue: &mut FormatResidue,
    settle: bool,
) -> Result<ScannedSector, StoreError<F::Error>> {
    let size = log.sector_size;
    let mut out = ScannedSector {
        end: SECTOR_HEADER_LEN,
        closed: false,
    };
    let mut off = SECTOR_HEADER_LEN;
    let mut payload = Vec::new();
    // The last record that checked (its header; `payload` holds its payload
    // while nothing after it is read), and whether the scan stopped at a
    // record that did not check (a torn program, which can read whole later).
    let mut last = None;
    let mut torn = false;
    while off + RECORD_HEADER_LEN <= size {
        let mut h = [0u8; RECORD_HEADER_LEN as usize];
        log.read(log.addr(s, off), &mut h)?;
        let (len, kind, root) = match RecordHeader::parse(&h) {
            HeaderRead::End => break,
            HeaderRead::Bad => {
                (out.closed, torn) = (true, true);
                break;
            }
            // Checked like any record, then skipped: garbage (FORMAT.md
            // "Unknown records").
            HeaderRead::Unknown { len } => (len, None, None),
            HeaderRead::Record(r) => (
                r.len,
                Some(r.kind),
                Some(r).filter(|r| r.kind == RecordKind::Root),
            ),
        };
        let total = RECORD_HEADER_LEN + u32::from(len);
        if off + total > size {
            (out.closed, torn) = (true, true);
            break;
        }
        payload.clear();
        payload.resize(usize::from(len), 0);
        log.read(log.addr(s, off + RECORD_HEADER_LEN), &mut payload)?;
        if !mutant!(TrustRecordHeaders) && !RecordHeader::crc_ok(&h, &payload) {
            (out.closed, torn) = (true, true);
            break;
        }
        residue.saw_record(kind, &payload);
        let loc = RecordLoc {
            sector: s,
            offset: off,
        };
        if let Some(r) = root {
            match RootRecord::decode(&payload) {
                Some(root) => roots.offer(RootCandidate {
                    seq: root.seq,
                    id: r.id,
                    loc,
                }),
                None => {
                    out.closed = true;
                    break;
                }
            }
        }
        last = Some(h);
        off += total;
        out.end = off;
    }
    if settle && !mutant!(TrustTailOnce) {
        if torn {
            log.kill_record(s, out.end)?;
        } else if !out.closed {
            // A sector holding no record is not resumed: its header's magic
            // (programmed last) is then its last program, and a head opens
            // a fresh sector instead of settling it. Only a cut between a
            // head's open and its first record leaves one.
            out.closed = match last {
                Some(h) => {
                    let at = out.end - RECORD_HEADER_LEN - payload.len() as u32;
                    !log.reprogram(log.addr(s, at), &h, &[&payload])?
                }
                None => true,
            };
        }
    }
    log.note(payload.capacity());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(seq: u64, id: u64) -> RootCandidate {
        RootCandidate {
            seq,
            id: ObjectId(id),
            loc: RecordLoc {
                sector: 0,
                offset: 24,
            },
        }
    }

    #[test]
    fn keeps_the_newest_two_roots() {
        let mut r = RootCandidates::default();
        for (seq, id) in [(3, 30), (7, 70), (5, 50), (1, 10), (7, 70)] {
            r.offer(cand(seq, id));
        }
        assert_eq!(r.best[0].unwrap().seq, 7);
        assert_eq!(r.best[1].unwrap().seq, 5);
        assert_eq!(r.max_seq, 7);
    }
}
