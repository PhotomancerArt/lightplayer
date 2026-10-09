//! The host's side of a deflated push (feature `host-deflate`, never on a
//! device): split a file into chunks of at most 4 KiB logical whose raw
//! deflate (`miniz_oxide`, level 10) fits one record of the board's
//! `record_max`, each with the id the board will compute. Feed them to
//! `put_chunk_deflated` in order (offset 0, then the running size).

use alloc::vec::Vec;

use crate::blob_codec::{DEFLATE_PREFIX, MAX_LOGICAL_CHUNK};
use crate::object_hasher::ObjectHasher;
use crate::object_id::{IdTag, ObjectId};
use crate::record_header::RECORD_HEADER_LEN;

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
    let room = (record_max - RECORD_HEADER_LEN) as usize - DEFLATE_PREFIX;
    let floor = room + DEFLATE_PREFIX;
    let mut out = Vec::new();
    let mut pos = 0;
    loop {
        let rem = &bytes[pos..];
        let mut n = rem.len().min(MAX_LOGICAL_CHUNK);
        let z = loop {
            let z = miniz_oxide::deflate::compress_to_vec(&rem[..n], 10);
            if z.len() <= room || n <= floor.min(rem.len()) {
                break z;
            }
            // Shrink in proportion, never below one stored record's worth.
            let guess = n * room * 15 / 16 / z.len().max(1);
            n = guess.clamp(floor.min(rem.len()), n - 1);
        };
        out.push(HostChunk {
            logical_len: n as u32,
            id: ObjectId::of(hasher, IdTag::Blob, &[&rem[..n]]),
            deflated: z,
        });
        pos += n;
        if pos >= bytes.len() {
            return out;
        }
    }
}
