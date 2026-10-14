//! The host's side of a deflated push with ids (feature `host-deflate`,
//! never on a device): [`crate::plan_deflated_chunks`]'s chunks, each with
//! the id the board will compute. Feed them to `put_chunk_deflated` in order
//! (offset 0, then the running size). A wire push needs no ids and uses the
//! plan alone.

use alloc::vec::Vec;

use crate::deflate_plan::{DEFAULT_DEFLATE_LEVEL, plan_deflated_chunks};
use crate::object_hasher::ObjectHasher;
use crate::object_id::{IdTag, ObjectId};

/// One chunk as the host sends it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostChunk {
    pub logical_len: u32,
    pub id: ObjectId,
    pub deflated: Vec<u8>,
}

/// `bytes` as deflated chunks for a board with `record_max`. A chunk that
/// will not shrink is still sent deflated (the board stores it as plain
/// bytes); it is never larger than `record_max` logical.
pub fn host_deflate_chunks<H: ObjectHasher>(
    hasher: &mut H,
    bytes: &[u8],
    record_max: u32,
) -> Vec<HostChunk> {
    plan_deflated_chunks(bytes, record_max, DEFAULT_DEFLATE_LEVEL)
        .into_iter()
        .map(|chunk| HostChunk {
            logical_len: chunk.logical_range.len() as u32,
            id: ObjectId::of(hasher, IdTag::Blob, &[&bytes[chunk.logical_range]]),
            deflated: chunk.deflated,
        })
        .collect()
}
