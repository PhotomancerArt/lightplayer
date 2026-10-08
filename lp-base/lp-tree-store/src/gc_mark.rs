//! Mark (invariant I3): live = everything reachable from the given roots.
//! The mark set is a bitset over index positions; each visited record's
//! header is read (its length goes into the sector's exact live bytes), and
//! the payloads of records that name others (roots, directories, multis)
//! are read and CRC-checked. A missing or unparsable record is `Corrupt` —
//! at mount that means "this root's closure is incomplete".
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
    /// Every live record's total length, when asked for (the packing bound).
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
        lens: Vec::new(),
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
        let h = log.header_at(loc)?;
        if h.id != id {
            return Err(StoreError::Corrupt("record id"));
        }
        let l = &mut m.live[loc.sector as usize];
        *l = l.saturating_add(h.total_len() as u16);
        if want_lens {
            m.lens.push(h.total_len() as u16);
        }
        match (role, h.kind) {
            (MarkRole::Node, RecordKind::Blob) => {}
            (MarkRole::Root, RecordKind::Root) => {
                let (_, p) = log.read_record(id)?;
                let r = RootRecord::decode(&p).ok_or(StoreError::Corrupt("root"))?;
                stack.push((r.cold_dir, MarkRole::Dir));
                stack.push((r.hot_dir, MarkRole::Dir));
            }
            (MarkRole::Dir, RecordKind::Dir) => {
                let (_, p) = log.read_record(id)?;
                push_entries(&p, &mut stack)?;
            }
            (MarkRole::Node | MarkRole::Dir, RecordKind::Multi) => {
                let (_, p) = log.read_record(id)?;
                let head = parse_multi(&p).ok_or(StoreError::Corrupt("multi"))?;
                for i in 0..head.count {
                    stack.push((multi_child(&p, i), MarkRole::Node));
                }
                if role == MarkRole::Dir {
                    if !head.dir {
                        return Err(StoreError::Corrupt("dir multi without the dir flag"));
                    }
                    dir_bytes.clear();
                    read_node_into(log, id, &mut dir_bytes)?;
                    push_entries(&dir_bytes, &mut stack)?;
                }
            }
            _ => return Err(StoreError::Corrupt("record kind")),
        }
        peak = peak.max(stack.capacity());
    }
    log.note(
        m.bits.len() * 4 + peak * core::mem::size_of::<(ObjectId, MarkRole)>() + dir_bytes.capacity(),
    );
    Ok(m)
}

fn push_entries<E>(
    dir: &[u8],
    stack: &mut Vec<(ObjectId, MarkRole)>,
) -> Result<(), StoreError<E>> {
    for e in decode_dir(dir).ok_or(StoreError::Corrupt("dir"))? {
        let role = match e.kind {
            EntryKind::File => MarkRole::Node,
            EntryKind::Dir => MarkRole::Dir,
        };
        stack.push((e.id, role));
    }
    Ok(())
}

/// Keep only the marked records in the index, and adopt the exact live
/// bytes.
pub fn prune<F: Flash>(log: &mut RecordLog<F>, m: Marked) {
    log.index.retain_positions(|i| m.is_live(i));
    log.index.shrink();
    log.sectors.live = m.live;
}
