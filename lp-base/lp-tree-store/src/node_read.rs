//! Reading nodes back: a node's logical bytes (decoded into the caller's
//! buffer), its leaf list (for an append), and a directory's entries. Every
//! record read is CRC-checked; a multi's levels and lengths are checked.

use alloc::vec::Vec;

use crate::blob_codec::{DEFLATE_PREFIX, decode_blob_into, logical_len};
use crate::dir_node::{DirEntry, decode_dir};
use crate::flash::Flash;
use crate::multi_node::{multi_child, parse_multi};
use crate::object_id::ObjectId;
use crate::record_kind::{ChunkCodec, RecordKind};
use crate::record_log::RecordLog;
use crate::store_error::StoreError;

type R<T, F> = Result<T, StoreError<<F as Flash>::Error>>;

/// One chunk of a node, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Leaf {
    pub id: ObjectId,
    /// Logical bytes.
    pub len: u32,
    /// Stored (so an append may re-chunk it).
    pub stored: bool,
}

/// Append node `id`'s logical bytes to `out`.
pub fn read_node_into<F: Flash>(
    log: &mut RecordLog<F>,
    id: ObjectId,
    out: &mut Vec<u8>,
) -> R<(), F> {
    let (h, payload) = log.read_record(id)?;
    match h.kind {
        RecordKind::Blob => decode_blob_into(h.codec, &payload, out)
            .ok_or(StoreError::Corrupt("chunk does not decode")),
        RecordKind::Dir => {
            out.extend_from_slice(&payload);
            Ok(())
        }
        RecordKind::Multi => {
            let start = out.len();
            let total = read_multi_into(log, &payload, None, out)?;
            if out.len() - start != total as usize {
                return Err(StoreError::Corrupt("multi length"));
            }
            Ok(())
        }
        RecordKind::Root => Err(StoreError::Corrupt("root is not a node")),
    }
}

/// A multi payload's children into `out`; `level` = the level its parent
/// expects (`None` at the top). Returns its total length.
fn read_multi_into<F: Flash>(
    log: &mut RecordLog<F>,
    payload: &[u8],
    level: Option<u8>,
    out: &mut Vec<u8>,
) -> R<u32, F> {
    let m = parse_multi(payload).ok_or(StoreError::Corrupt("multi"))?;
    if level.is_some_and(|l| l != m.level) {
        return Err(StoreError::Corrupt("multi level"));
    }
    let start = out.len();
    for i in 0..m.count {
        let (h, p) = log.read_record(multi_child(payload, i))?;
        match (m.level, h.kind) {
            (0, RecordKind::Blob) => decode_blob_into(h.codec, &p, out)
                .ok_or(StoreError::Corrupt("chunk does not decode"))?,
            (l, RecordKind::Multi) if l > 0 => {
                read_multi_into(log, &p, Some(l - 1), out)?;
            }
            _ => return Err(StoreError::Corrupt("multi child")),
        }
    }
    if out.len() - start != m.total_len as usize {
        return Err(StoreError::Corrupt("multi length"));
    }
    Ok(m.total_len)
}

/// Node `id`'s chunks, in order (headers and multis only; no chunk payload).
pub fn leaf_list<F: Flash>(log: &mut RecordLog<F>, id: ObjectId) -> R<Vec<Leaf>, F> {
    let mut out = Vec::new();
    leaves_of(log, id, Expect::Top, &mut out)?;
    log.note(out.capacity() * core::mem::size_of::<Leaf>());
    Ok(out)
}

/// What a parent says a child is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// A file node: a blob, or a multi of any level.
    Top,
    Blob,
    Multi(u8),
}

fn leaves_of<F: Flash>(
    log: &mut RecordLog<F>,
    id: ObjectId,
    expect: Expect,
    out: &mut Vec<Leaf>,
) -> R<(), F> {
    let loc = log
        .index
        .get(id)
        .ok_or(StoreError::Corrupt("missing record"))?;
    let h = log.header_at(loc)?;
    if h.id != id {
        return Err(StoreError::Corrupt("record id"));
    }
    match (expect, h.kind) {
        (Expect::Top | Expect::Blob, RecordKind::Blob) => {
            let len = match h.codec {
                ChunkCodec::Stored => usize::from(h.len),
                ChunkCodec::Deflate => {
                    let mut p = [0u8; DEFLATE_PREFIX];
                    log.read_payload_prefix(loc, &mut p)?;
                    logical_len(h.codec, &p).ok_or(StoreError::Corrupt("chunk length"))?
                }
            };
            out.push(Leaf {
                id,
                len: len as u32,
                stored: h.codec == ChunkCodec::Stored,
            });
            Ok(())
        }
        (Expect::Top | Expect::Multi(_), RecordKind::Multi) => {
            let (_, p) = log.read_record(id)?;
            let m = parse_multi(&p).ok_or(StoreError::Corrupt("multi"))?;
            if matches!(expect, Expect::Multi(l) if l != m.level) {
                return Err(StoreError::Corrupt("multi level"));
            }
            let child = match m.level {
                0 => Expect::Blob,
                l => Expect::Multi(l - 1),
            };
            for i in 0..m.count {
                leaves_of(log, multi_child(&p, i), child, out)?;
            }
            Ok(())
        }
        _ => Err(StoreError::Corrupt("not a file node")),
    }
}

/// A directory node's entries.
pub fn read_dir<F: Flash>(log: &mut RecordLog<F>, id: ObjectId) -> R<Vec<DirEntry>, F> {
    let mut bytes = Vec::new();
    read_node_into(log, id, &mut bytes)?;
    log.note(bytes.capacity());
    decode_dir(&bytes).ok_or(StoreError::Corrupt("dir"))
}
