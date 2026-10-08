//! Mount's scan of one sector: every record header and payload, CRC-checked,
//! stopping at the end or at the first record that does not check.

use alloc::vec;
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

/// What one sector holds.
#[derive(Clone, Debug, Default)]
pub struct ScannedSector {
    /// Offset just past the last good record.
    pub end: u32,
    /// A record failed to check: nothing after it is trusted.
    pub closed: bool,
    pub records: Vec<(ObjectId, RecordLoc)>,
    /// `(seq, id)` of every good `Root` record.
    pub roots: Vec<(u64, ObjectId)>,
}

/// Scan sector `s`, whose header was valid.
pub fn scan_sector<F: Flash>(
    log: &mut RecordLog<F>,
    s: u32,
) -> Result<ScannedSector, StoreError<F::Error>> {
    let size = log.sector_size;
    let mut out = ScannedSector {
        end: SECTOR_HEADER_LEN,
        ..ScannedSector::default()
    };
    let mut off = SECTOR_HEADER_LEN;
    let mut payload = Vec::new();
    while off + RECORD_HEADER_LEN <= size {
        let mut h = [0u8; RECORD_HEADER_LEN as usize];
        log.read(log.addr(s, off), &mut h)?;
        let r = match RecordHeader::parse(&h) {
            HeaderRead::End => break,
            HeaderRead::Bad => {
                out.closed = true;
                break;
            }
            HeaderRead::Record(r) => r,
        };
        let total = RECORD_HEADER_LEN + u32::from(r.len);
        if off + total > size {
            out.closed = true;
            break;
        }
        payload.clear();
        payload.resize(usize::from(r.len), 0);
        log.read(log.addr(s, off + RECORD_HEADER_LEN), &mut payload)?;
        if !RecordHeader::crc_ok(&h, &payload) {
            out.closed = true;
            break;
        }
        if r.kind == RecordKind::Root {
            match RootRecord::decode(&payload) {
                Some(root) => out.roots.push((root.seq, r.id)),
                None => {
                    out.closed = true;
                    break;
                }
            }
        }
        out.records.push((
            r.id,
            RecordLoc {
                sector: s as u16,
                offset: off as u16,
                len: r.len,
                kind: r.kind,
                codec: r.codec,
            },
        ));
        off += total;
        out.end = off;
    }
    if out.closed {
        out.end = size;
    }
    Ok(out)
}

/// Whether `[from .. sector end)` of `s` reads all `0xFF` (a resumable head).
pub fn tail_is_erased<F: Flash>(
    log: &mut RecordLog<F>,
    s: u32,
    from: u32,
) -> Result<bool, StoreError<F::Error>> {
    let mut buf = vec![0u8; 256];
    let mut off = from;
    while off < log.sector_size {
        let n = (log.sector_size - off).min(256) as usize;
        log.read(log.addr(s, off), &mut buf[..n])?;
        if buf[..n].iter().any(|&b| b != 0xFF) {
            return Ok(false);
        }
        off += n as u32;
    }
    Ok(true)
}
