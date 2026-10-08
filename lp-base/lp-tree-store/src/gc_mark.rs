//! Mark (invariant I3): live = everything reachable from the given roots,
//! following each record's references. Records not on flash yet are read
//! from the commit's plan. A missing or unparsable record is
//! `Corrupt` — at mount that means "this root's closure is incomplete".

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use crate::blob_codec::chunk_dict_ref;
use crate::dir_node::decode_dir;
use crate::flash::Flash;
use crate::multi_node::MultiNode;
use crate::object_id::ObjectId;
use crate::record_kind::{ChunkCodec, RecordKind};
use crate::record_log::RecordLog;
use crate::record_plan::RecordPlan;
use crate::root_record::RootRecord;
use crate::store_error::StoreError;

/// Every id reachable from `roots`.
pub fn mark<F: Flash>(
    log: &mut RecordLog<F>,
    roots: &[ObjectId],
    plan: &RecordPlan,
) -> Result<BTreeSet<ObjectId>, StoreError<F::Error>> {
    let mut live = BTreeSet::new();
    let mut stack: Vec<ObjectId> = roots.iter().copied().filter(|id| !id.is_none()).collect();
    while let Some(id) = stack.pop() {
        if !live.insert(id) {
            continue;
        }
        let (kind, codec, payload) = if let Some(r) = plan.get(id) {
            (r.kind, r.codec, r.payload.clone())
        } else {
            let loc = log
                .index
                .get(id)
                .ok_or(StoreError::Corrupt("missing record"))?;
            let payload = match loc.kind {
                RecordKind::Blob if loc.codec == ChunkCodec::DeflateDict => {
                    log.read_payload_prefix(loc, 10)?
                }
                RecordKind::Blob | RecordKind::Dict => Vec::new(),
                _ => log.read_payload(id)?.1,
            };
            (loc.kind, loc.codec, payload)
        };
        refs_of(kind, codec, &payload, &mut stack)?;
    }
    Ok(live)
}

/// Push the ids a record names.
fn refs_of<E>(
    kind: RecordKind,
    codec: ChunkCodec,
    payload: &[u8],
    out: &mut Vec<ObjectId>,
) -> Result<(), StoreError<E>> {
    match kind {
        RecordKind::Blob => out.extend(chunk_dict_ref(codec, payload)),
        RecordKind::Dict => {}
        RecordKind::Multi => {
            let m = MultiNode::decode(payload).ok_or(StoreError::Corrupt("multi"))?;
            out.extend(m.children);
        }
        RecordKind::Dir => {
            let d = decode_dir(payload).ok_or(StoreError::Corrupt("dir"))?;
            out.extend(d.into_iter().map(|e| e.id));
        }
        RecordKind::Root => {
            let r = RootRecord::decode(payload).ok_or(StoreError::Corrupt("root"))?;
            out.extend(r.refs().into_iter().filter(|id| !id.is_none()));
        }
    }
    Ok(())
}

/// Recompute every sector's live bytes: the index's copy of each live id.
pub fn recount_live_bytes<F: Flash>(log: &mut RecordLog<F>, live: &BTreeSet<ObjectId>) {
    log.sectors.live_bytes.iter_mut().for_each(|b| *b = 0);
    let counts: Vec<(usize, u32)> = log
        .index
        .iter()
        .filter(|(id, _)| live.contains(id))
        .map(|(_, loc)| (usize::from(loc.sector), loc.total_len()))
        .collect();
    for (s, n) in counts {
        log.sectors.live_bytes[s] += n;
    }
}
