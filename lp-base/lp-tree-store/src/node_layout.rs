//! Laying a logical node (a file, a directory, the dictionary) out as
//! records that each fit `record_max`: one record when it fits, else chunks
//! under a tree of `Multi` records whose top carries the node's id.
//!
//! Ids: the node's id is `H(tag ++ its logical bytes)` whatever its shape;
//! a chunk is `H(Chunk ++ chunk bytes)`; an inner multi `H(MultiInner ++ its
//! payload)`. Pure: no flash, no dedup (the caller skips known ids).

use alloc::vec;
use alloc::vec::Vec;

use crate::blob_codec::{MAX_LOGICAL_CHUNK, deflate_payload, prefix_len};
use crate::multi_node::MultiNode;
use crate::object_id::{IdTag, ObjectId};
use crate::record_header::RECORD_HEADER_LEN;
use crate::record_kind::{ChunkCodec, RecordKind};
use crate::store_config::Codec;

/// What a node is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeTag {
    File,
    Dir,
    /// Only the encoder trains (and so lays out) a dictionary.
    #[cfg(feature = "encode")]
    Dict,
}

impl NodeTag {
    pub fn id_tag(self) -> IdTag {
        match self {
            NodeTag::File => IdTag::File,
            NodeTag::Dir => IdTag::Dir,
            #[cfg(feature = "encode")]
            NodeTag::Dict => IdTag::Dict,
        }
    }

    /// The record kind of the one-record form.
    pub fn single_kind(self) -> RecordKind {
        match self {
            NodeTag::File => RecordKind::Blob,
            NodeTag::Dir => RecordKind::Dir,
            #[cfg(feature = "encode")]
            NodeTag::Dict => RecordKind::Dict,
        }
    }
}

/// How chunks are coded.
#[derive(Clone, Copy, Debug)]
pub struct LayoutCtx<'a> {
    pub record_max: u32,
    pub codec: Codec,
    /// The dictionary for `DeflateDict` chunks.
    pub dict: Option<(ObjectId, &'a [u8])>,
}

/// One record to write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaidRecord {
    pub id: ObjectId,
    pub kind: RecordKind,
    pub codec: ChunkCodec,
    pub payload: Vec<u8>,
}

/// The node's id and its records, children before parents (the top last).
pub fn layout_node(tag: NodeTag, bytes: &[u8], ctx: &LayoutCtx<'_>) -> (ObjectId, Vec<LaidRecord>) {
    let node_id = ObjectId::of(tag.id_tag(), bytes);
    let max_payload = (ctx.record_max - RECORD_HEADER_LEN) as usize;
    let coded = tag == NodeTag::File;
    if let Some((codec, payload)) = code_whole(ctx, bytes, max_payload, coded) {
        let rec = LaidRecord {
            id: node_id,
            kind: tag.single_kind(),
            codec,
            payload,
        };
        return (node_id, vec![rec]);
    }
    let mut out = Vec::new();
    let mut level_nodes: Vec<(ObjectId, u32)> = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        let (n, codec, payload) = next_chunk(ctx, &bytes[pos..], max_payload, coded);
        let id = ObjectId::of(IdTag::Chunk, &bytes[pos..pos + n]);
        out.push(LaidRecord {
            id,
            kind: RecordKind::Blob,
            codec,
            payload,
        });
        level_nodes.push((id, n as u32));
        pos += n;
    }
    let fanout = MultiNode::fanout(max_payload).max(2);
    let mut level = 0u8;
    loop {
        if level_nodes.len() <= fanout {
            let m = MultiNode {
                level,
                total_len: level_nodes.iter().map(|x| x.1).sum(),
                children: level_nodes.iter().map(|x| x.0).collect(),
            };
            out.push(LaidRecord {
                id: node_id,
                kind: RecordKind::Multi,
                codec: ChunkCodec::Stored,
                payload: m.encode(),
            });
            return (node_id, out);
        }
        let mut next = Vec::new();
        for group in level_nodes.chunks(fanout) {
            let m = MultiNode {
                level,
                total_len: group.iter().map(|x| x.1).sum(),
                children: group.iter().map(|x| x.0).collect(),
            };
            let payload = m.encode();
            let id = ObjectId::of(IdTag::MultiInner, &payload);
            out.push(LaidRecord {
                id,
                kind: RecordKind::Multi,
                codec: ChunkCodec::Stored,
                payload,
            });
            next.push((id, m.total_len));
        }
        level_nodes = next;
        level += 1;
    }
}

/// The whole node as one payload, if it fits.
fn code_whole(
    ctx: &LayoutCtx<'_>,
    bytes: &[u8],
    max_payload: usize,
    coded: bool,
) -> Option<(ChunkCodec, Vec<u8>)> {
    if coded
        && let Some((c, p)) = try_deflate(ctx, bytes)
        && p.len() <= max_payload
        && p.len() < bytes.len()
    {
        return Some((c, p));
    }
    (bytes.len() <= max_payload && bytes.len() <= MAX_LOGICAL_CHUNK)
        .then(|| (ChunkCodec::Stored, bytes.to_vec()))
}

/// The next chunk from `rem`: (logical length, codec, payload).
fn next_chunk(
    ctx: &LayoutCtx<'_>,
    rem: &[u8],
    max_payload: usize,
    coded: bool,
) -> (usize, ChunkCodec, Vec<u8>) {
    let floor = rem.len().min(max_payload);
    let stored = |n: usize| (n, ChunkCodec::Stored, rem[..n].to_vec());
    if !coded || ctx.codec == Codec::Stored {
        return stored(floor);
    }
    let mut n = rem.len().min(MAX_LOGICAL_CHUNK);
    loop {
        match try_deflate(ctx, &rem[..n]) {
            Some((c, p)) if p.len() <= max_payload && p.len() < n => return (n, c, p),
            Some((c, p)) if n > floor => {
                let room = (max_payload - prefix_len(c)) * 15 / 16;
                let guess = (n as u64 * room as u64 / p.len().max(1) as u64) as usize;
                n = guess.clamp(floor, n - 1);
            }
            _ => return stored(floor),
        }
    }
}

fn try_deflate(ctx: &LayoutCtx<'_>, bytes: &[u8]) -> Option<(ChunkCodec, Vec<u8>)> {
    match ctx.codec {
        Codec::Stored => None,
        Codec::Deflate => deflate_payload(bytes, None),
        Codec::DeflateDict => deflate_payload(bytes, ctx.dict.filter(|d| !d.1.is_empty())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blob_codec::decode_chunk;

    fn ctx(codec: Codec) -> LayoutCtx<'static> {
        LayoutCtx {
            record_max: 256,
            codec,
            dict: None,
        }
    }

    #[test]
    fn small_is_one_record_big_is_multi_tree() {
        let (id, recs) = layout_node(NodeTag::File, b"tiny", &ctx(Codec::Stored));
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, id);
        let big: Vec<u8> = (0..20_000u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
            .collect();
        let (id, recs) = layout_node(NodeTag::File, &big, &ctx(Codec::Stored));
        let top = recs.last().unwrap();
        assert_eq!((top.id, top.kind), (id, RecordKind::Multi));
        assert!(recs.iter().all(|r| r.payload.len() + 16 <= 256));
        assert!(recs.iter().filter(|r| r.kind == RecordKind::Multi).count() > 1);
        let joined: Vec<u8> = recs
            .iter()
            .filter(|r| r.kind == RecordKind::Blob)
            .flat_map(|r| decode_chunk(r.codec, &r.payload, None).unwrap())
            .collect();
        assert_eq!(joined, big);
    }

    #[cfg(feature = "encode")]
    #[test]
    fn deflate_chunks_fit_and_decode() {
        let text = b"{\"name\": \"orbit\", \"speed\": 0.25, \"palette\": [1, 2, 3]}\n".repeat(200);
        let (_, recs) = layout_node(NodeTag::File, &text, &ctx(Codec::Deflate));
        assert!(recs.iter().all(|r| r.payload.len() + 16 <= 256));
        let joined: Vec<u8> = recs
            .iter()
            .filter(|r| r.kind == RecordKind::Blob)
            .flat_map(|r| decode_chunk(r.codec, &r.payload, None).unwrap())
            .collect();
        assert_eq!(joined, text);
    }
}
