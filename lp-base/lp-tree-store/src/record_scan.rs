//! Mount's first pass over one sector: every record header and payload,
//! CRC-checked (a torn program can leave a good header over a short payload
//! — README defect 1), stopping at the end or at the first record that does
//! not check; a record of an unknown kind is checked the same way and
//! skipped. Nothing is indexed here (that would cost RAM for every record
//! on flash, garbage included): the pass finds where each sector's trusted
//! records end and keeps the roots, two at most (I1 falls back one step and
//! no further). `mount_walk.rs` then indexes only the chosen root's closure.

use alloc::vec::Vec;

use crate::flash::Flash;
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

/// Scan sector `s`, whose header was valid.
pub fn scan_sector<F: Flash>(
    log: &mut RecordLog<F>,
    s: u32,
    roots: &mut RootCandidates,
) -> Result<ScannedSector, StoreError<F::Error>> {
    let size = log.sector_size;
    let mut out = ScannedSector {
        end: SECTOR_HEADER_LEN,
        closed: false,
    };
    let mut off = SECTOR_HEADER_LEN;
    let mut payload = Vec::new();
    while off + RECORD_HEADER_LEN <= size {
        let mut h = [0u8; RECORD_HEADER_LEN as usize];
        log.read(log.addr(s, off), &mut h)?;
        let (len, root) = match RecordHeader::parse(&h) {
            HeaderRead::End => break,
            HeaderRead::Bad => {
                out.closed = true;
                break;
            }
            // Checked like any record, then skipped: garbage (FORMAT.md
            // "Unknown records").
            HeaderRead::Unknown { len } => (len, None),
            HeaderRead::Record(r) => (r.len, Some(r).filter(|r| r.kind == RecordKind::Root)),
        };
        let total = RECORD_HEADER_LEN + u32::from(len);
        if off + total > size {
            out.closed = true;
            break;
        }
        payload.clear();
        payload.resize(usize::from(len), 0);
        log.read(log.addr(s, off + RECORD_HEADER_LEN), &mut payload)?;
        if !mutant!(TrustRecordHeaders) && !RecordHeader::crc_ok(&h, &payload) {
            out.closed = true;
            break;
        }
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
        off += total;
        out.end = off;
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
