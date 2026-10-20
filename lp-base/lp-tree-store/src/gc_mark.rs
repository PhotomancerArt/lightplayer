//! Mark (invariant I3): live = everything reachable from the given roots.
//! The mark set is a bitset over index positions; each visited record's
//! header is read (its length goes into the sector's exact live bytes), and
//! the payloads of records that name others (roots, directories, multis)
//! are read and CRC-checked. A missing or unparsable record is `Corrupt`.
//! Mount marks with the same per-record step ([`visit`]) while it builds the
//! index (`mount_walk.rs`), so both agree on what a complete closure is.
//!
//! A directory too big for one record is a flagged `Multi` whose children
//! are chunks of the directory's bytes: mark reassembles those bytes to find
//! the entries (the prototype did not, so a big directory's files were
//! never marked live).

use alloc::vec;
use alloc::vec::Vec;

use crate::dir_node::{EntryKind, decode_dir};
use crate::flash::Flash;
use crate::multi_node::{multi_child, parse_multi};
use crate::node_read::read_node_into;
use crate::object_id::ObjectId;
use crate::ram_index::RecordLoc;
use crate::record_header::{HeaderRead, RECORD_HEADER_LEN, RecordHeader};
use crate::record_kind::RecordKind;
use crate::record_log::RecordLog;
use crate::root_record::RootRecord;
use crate::store_error::StoreError;

/// What a reached id is expected to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkRole {
    Root,
    /// A directory node (a `Dir` record, or a `Multi` with the dir flag).
    Dir,
    /// A file node, a chunk, an inner multi.
    Node,
}

/// The result of a mark.
pub struct Marked {
    /// Bit `i` = index position `i` is live.
    pub bits: Vec<u32>,
    /// Exact live bytes per sector.
    pub live: Vec<u16>,
    /// When asked for: each index position's record length (0 = not live);
    /// after [`prune`], one per index entry, in index order (the packing
    /// bound's sizes and GC's).
    pub lens: Vec<u16>,
}

impl Marked {
    pub fn is_live(&self, pos: usize) -> bool {
        self.bits[pos / 32] & (1 << (pos % 32)) != 0
    }
}

/// Every record reachable from `roots`.
pub fn mark<F: Flash>(
    log: &mut RecordLog<F>,
    roots: &[(ObjectId, MarkRole)],
    want_lens: bool,
) -> Result<Marked, StoreError<F::Error>> {
    let n = log.index.len();
    let mut m = Marked {
        bits: vec![0u32; n.div_ceil(32)],
        live: vec![0u16; log.sector_count as usize],
        lens: vec![0u16; if want_lens { n } else { 0 }],
    };
    let mut stack: Vec<(ObjectId, MarkRole)> =
        roots.iter().copied().filter(|r| !r.0.is_none()).collect();
    let mut peak = 0;
    let mut dir_bytes = Vec::new();
    while let Some((id, role)) = stack.pop() {
        let pos = log
            .index
            .find(id)
            .map_err(|_| StoreError::Corrupt("missing record"))?;
        if m.is_live(pos) {
            continue;
        }
        m.bits[pos / 32] |= 1 << (pos % 32);
        let loc = log.index.loc_at(pos);
        let h = visit(
            log,
            id,
            role,
            loc,
            &mut VisitOut {
                stack: &mut stack,
                dir_bytes: &mut dir_bytes,
                defer: None,
            },
        )?;
        let l = &mut m.live[loc.sector as usize];
        *l = l.saturating_add(h.total_len() as u16);
        if want_lens {
            m.lens[pos] = h.total_len() as u16;
        }
        peak = peak.max(stack.capacity());
    }
    log.note(
        m.bits.len() * 4
            + m.lens.capacity() * 2
            + peak * core::mem::size_of::<(ObjectId, MarkRole)>()
            + dir_bytes.capacity(),
    );
    Ok(m)
}

/// Where a visit puts what it finds. `defer`: a directory multi's entries
/// are not read now (its chunks may not be indexed yet): its id goes there,
/// and the caller reads them once its chunks are.
pub(crate) struct VisitOut<'a> {
    pub stack: &'a mut Vec<(ObjectId, MarkRole)>,
    pub dir_bytes: &'a mut Vec<u8>,
    pub defer: Option<&'a mut Vec<ObjectId>>,
}

/// One reached record (at `loc`): check it is what its parent says it is,
/// and push every id it names.
pub(crate) fn visit<F: Flash>(
    log: &mut RecordLog<F>,
    id: ObjectId,
    role: MarkRole,
    loc: RecordLoc,
    out: &mut VisitOut<'_>,
) -> Result<RecordHeader, StoreError<F::Error>> {
    let mut raw = [0u8; RECORD_HEADER_LEN as usize];
    log.read(log.addr(loc.sector, loc.offset), &mut raw)?;
    visit_header(log, id, role, loc, &raw, out)
}

/// [`visit`] of a record whose header bytes `raw` the caller has read: only
/// its payload is read (once, CRC-checked), and only when it names others:
/// a blob's is checked when it is read.
pub(crate) fn visit_header<F: Flash>(
    log: &mut RecordLog<F>,
    id: ObjectId,
    role: MarkRole,
    loc: RecordLoc,
    raw: &[u8; RECORD_HEADER_LEN as usize],
    out: &mut VisitOut<'_>,
) -> Result<RecordHeader, StoreError<F::Error>> {
    let HeaderRead::Record(h) = RecordHeader::parse(raw) else {
        return Err(StoreError::Corrupt("record header"));
    };
    if h.id != id {
        return Err(StoreError::Corrupt("record id"));
    }
    match (role, h.kind) {
        (MarkRole::Node, RecordKind::Blob) => {}
        (MarkRole::Root, RecordKind::Root) => {
            let p = log.read_payload(loc, raw)?;
            let r = RootRecord::decode(&p).ok_or(StoreError::Corrupt("root"))?;
            out.stack.push((r.cold_dir, MarkRole::Dir));
            out.stack.push((r.hot_dir, MarkRole::Dir));
        }
        (MarkRole::Dir, RecordKind::Dir) => {
            let p = log.read_payload(loc, raw)?;
            push_entries(&p, out.stack)?;
        }
        (MarkRole::Node | MarkRole::Dir, RecordKind::Multi) => {
            let p = log.read_payload(loc, raw)?;
            let head = parse_multi(&p).ok_or(StoreError::Corrupt("multi"))?;
            for i in 0..head.count {
                out.stack.push((multi_child(&p, i), MarkRole::Node));
            }
            if role == MarkRole::Dir {
                if !head.dir {
                    return Err(StoreError::Corrupt("dir multi without the dir flag"));
                }
                match &mut out.defer {
                    Some(later) => later.push(id),
                    None => push_dir_node_entries(log, id, out.dir_bytes, out.stack)?,
                }
            }
        }
        _ => return Err(StoreError::Corrupt("record kind")),
    }
    Ok(h)
}

/// A directory node's entries (its bytes reassembled into `dir_bytes`)
/// onto `stack`.
pub(crate) fn push_dir_node_entries<F: Flash>(
    log: &mut RecordLog<F>,
    id: ObjectId,
    dir_bytes: &mut Vec<u8>,
    stack: &mut Vec<(ObjectId, MarkRole)>,
) -> Result<(), StoreError<F::Error>> {
    dir_bytes.clear();
    read_node_into(log, id, dir_bytes)?;
    push_entries(dir_bytes, stack)
}

fn push_entries<E>(dir: &[u8], stack: &mut Vec<(ObjectId, MarkRole)>) -> Result<(), StoreError<E>> {
    for e in decode_dir(dir).ok_or(StoreError::Corrupt("dir"))? {
        let role = match e.kind {
            EntryKind::File => MarkRole::Node,
            EntryKind::Dir => MarkRole::Dir,
        };
        stack.push((e.id, role));
    }
    Ok(())
}

/// Keep only the marked records in the index (and their lengths, when the
/// mark took them: a live record's is never 0), and adopt the exact live
/// bytes.
pub fn prune<F: Flash>(log: &mut RecordLog<F>, m: &mut Marked) {
    log.index.retain_positions(|i| m.is_live(i));
    log.index.shrink();
    m.lens.retain(|&l| l != 0);
    log.sectors.live = core::mem::take(&mut m.live);
}
