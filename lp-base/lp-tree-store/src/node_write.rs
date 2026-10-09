//! Writing content: a file's bytes as stored chunks under a multi tree, an
//! append that re-chunks only the stored tail, a host-deflated chunk
//! (verified by inflating and hashing it), and a directory node. Every
//! record is skipped when its id is already indexed (dedup: the index's
//! records all have complete closures, `ram_index.rs`).

use alloc::vec;
use alloc::vec::Vec;

use crate::blob_codec::{DEFLATE_PREFIX, MAX_LOGICAL_CHUNK};
use crate::flash::Flash;
use crate::gc_mark::MarkRole;
use crate::multi_node::{encode_multi, multi_fanout};
use crate::node_read::{Leaf, leaf_list, read_node_into};
use crate::object_hasher::ObjectHasher;
use crate::object_id::{IdTag, ObjectId};
use crate::record_header::RECORD_HEADER_LEN;
use crate::record_kind::{ChunkCodec, RecordKind};
use crate::sector_header::HeadKind;
use crate::store_error::StoreError;
use crate::tree_delta::FileEntry;
use crate::tree_store::{Res, TreeStore, Txn, head_for};

const HDR: u32 = RECORD_HEADER_LEN;

impl<F: Flash, H: ObjectHasher> TreeStore<F, H> {
    pub(crate) fn put_inner(
        &mut self,
        path: &str,
        existed: Option<FileEntry>,
        bytes: &[u8],
    ) -> Res<(), F> {
        let head = head_for(path);
        let mut need = Vec::new();
        let chunks = self.stored_chunk_need(head, bytes.len(), &mut need);
        self.multi_need(head, chunks, false, &mut need);
        self.implicit_need(path, &mut need);
        self.ensure_room(&need)?;
        let mut leaves = Vec::new();
        self.write_stored_chunks(head, bytes, &[], &mut leaves)?;
        let id = self.write_tree(head, &leaves, false)?;
        let size = bytes.len() as u32;
        self.record_set(path, existed, FileEntry { id, size });
        Ok(())
    }

    pub(crate) fn append_inner(&mut self, path: &str, bytes: &[u8]) -> Res<(), F> {
        let existed = self.existing(path)?;
        let Some(fe) = existed else {
            return self.put_inner(path, None, bytes);
        };
        let size =
            u32::try_from(fe.size as usize + bytes.len()).map_err(|_| StoreError::TooLarge)?;
        let head = head_for(path);
        let mut leaves = leaf_list(&mut self.log, fe.id)?;
        let mut tail = Vec::new();
        if let Some(last) = leaves.last().copied()
            && last.stored
            && (last.len as usize) < self.max_payload()
        {
            read_node_into(&mut self.log, last.id, &mut tail)?;
            leaves.pop();
        }
        let mut need = Vec::new();
        let chunks = self.stored_chunk_need(head, tail.len() + bytes.len(), &mut need);
        self.multi_need(head, leaves.len() + chunks, false, &mut need);
        self.implicit_need(path, &mut need);
        self.ensure_room(&need)?;
        self.write_stored_chunks(head, &tail, bytes, &mut leaves)?;
        let id = self.write_tree(head, &leaves, false)?;
        self.record_set(path, existed, FileEntry { id, size });
        Ok(())
    }

    pub(crate) fn deflated_inner(
        &mut self,
        path: &str,
        offset: u32,
        logical_len: u32,
        expected: Option<ObjectId>,
        deflated: &[u8],
    ) -> Res<(), F> {
        let logical = logical_len as usize;
        if logical > MAX_LOGICAL_CHUNK {
            return Err(StoreError::TooLarge);
        }
        let existed = self.existing(path)?;
        let mut leaves = match (offset, existed) {
            (0, _) => Vec::new(),
            (o, Some(fe)) if fe.size == o => leaf_list(&mut self.log, fe.id)?,
            _ => return Err(StoreError::BadOffset),
        };
        let size = offset
            .checked_add(logical_len)
            .ok_or(StoreError::TooLarge)?;
        let mut buf = vec![0u8; logical];
        self.log.note(buf.len());
        match lp_deflate::inflate(deflated, &mut buf, 0) {
            Ok(n) if n == logical => {}
            _ => return Err(StoreError::Corrupt("chunk does not inflate to its length")),
        }
        let id = ObjectId::of(&mut self.hasher, IdTag::Blob, &[&buf]);
        if expected.is_some_and(|e| e != id) {
            return Err(StoreError::Corrupt("chunk id"));
        }
        let head = head_for(path);
        let coded = DEFLATE_PREFIX + deflated.len();
        let keep_deflated = coded <= self.max_payload() && coded < logical;
        let mut need = Vec::new();
        let chunks = if keep_deflated {
            need.push((head, HDR + coded as u32));
            1
        } else {
            self.stored_chunk_need(head, logical, &mut need)
        };
        self.multi_need(head, leaves.len() + chunks, false, &mut need);
        self.implicit_need(path, &mut need);
        self.ensure_room(&need)?;
        if keep_deflated {
            let len = (logical as u16).to_le_bytes();
            self.write_if_new(
                head,
                RecordKind::Blob,
                ChunkCodec::Deflate,
                id,
                &[&len, deflated],
            )?;
            leaves.push(Leaf {
                id,
                len: logical_len,
                stored: false,
            });
        } else {
            self.write_stored_chunks(head, &buf, &[], &mut leaves)?;
        }
        let node = self.write_tree(head, &leaves, false)?;
        self.record_set(path, existed, FileEntry { id: node, size });
        Ok(())
    }

    /// A directory node from its bytes: one `Dir` record, or a flagged
    /// multi over stored chunks. Makes room for exactly its records first
    /// (GC may run: what the flush wrote so far is in flight), and is in
    /// flight itself after.
    pub(crate) fn write_dir_bytes(&mut self, head: HeadKind, bytes: &[u8]) -> Res<ObjectId, F> {
        let mut need = Vec::new();
        let id = if bytes.len() <= self.max_payload() {
            let id = ObjectId::of(&mut self.hasher, IdTag::Dir, &[bytes]);
            if !self.log.index.contains(id) {
                need.push((head, HDR + bytes.len() as u32));
                self.ensure_room(&need)?;
            }
            self.write_if_new(head, RecordKind::Dir, ChunkCodec::Stored, id, &[bytes])?;
            id
        } else {
            let chunks = self.stored_chunk_need(head, bytes.len(), &mut need);
            self.multi_need(head, chunks, true, &mut need);
            self.ensure_room(&need)?;
            let mut leaves = Vec::new();
            self.write_stored_chunks(head, bytes, &[], &mut leaves)?;
            self.write_tree(head, &leaves, true)?
        };
        self.inflight.push((id, MarkRole::Dir));
        Ok(id)
    }

    /// Stored chunks of `a ++ b` (at most `record_max` each, so chunk
    /// boundaries sit at fixed offsets: a file built by appends has the
    /// chunks a single put of it would), each written unless known.
    pub(crate) fn write_stored_chunks(
        &mut self,
        head: HeadKind,
        a: &[u8],
        b: &[u8],
        leaves: &mut Vec<Leaf>,
    ) -> Res<(), F> {
        let total = a.len() + b.len();
        let mp = self.max_payload();
        let mut pos = 0;
        loop {
            let n = (total - pos).min(mp);
            let (s, e) = (pos, pos + n);
            let pa = &a[s.min(a.len())..e.min(a.len())];
            let pb = &b[s.saturating_sub(a.len())..e.saturating_sub(a.len())];
            let id = ObjectId::of(&mut self.hasher, IdTag::Blob, &[pa, pb]);
            self.write_if_new(head, RecordKind::Blob, ChunkCodec::Stored, id, &[pa, pb])?;
            leaves.push(Leaf {
                id,
                len: n as u32,
                stored: true,
            });
            pos = e;
            if pos >= total {
                break;
            }
        }
        self.log
            .note(leaves.capacity() * core::mem::size_of::<Leaf>());
        Ok(())
    }

    /// The node over `leaves`: the leaf itself when there is one (and it is
    /// not a directory), else a multi tree, `fanout` children per record,
    /// grouped from the left.
    pub(crate) fn write_tree(
        &mut self,
        head: HeadKind,
        leaves: &[Leaf],
        dir: bool,
    ) -> Res<ObjectId, F> {
        if leaves.len() == 1 && !dir {
            return Ok(leaves[0].id);
        }
        let fanout = multi_fanout(self.max_payload());
        let mut level = 0u8;
        let mut cur: Vec<(ObjectId, u32)> = leaves.iter().map(|l| (l.id, l.len)).collect();
        loop {
            if cur.len() <= fanout {
                return Ok(self.write_multi(head, level, dir, &cur)?.0);
            }
            let mut next = Vec::with_capacity(cur.len().div_ceil(fanout));
            for group in cur.chunks(fanout) {
                next.push(self.write_multi(head, level, dir, group)?);
            }
            self.log
                .note((cur.capacity() + next.capacity()) * core::mem::size_of::<(ObjectId, u32)>());
            cur = next;
            level += 1;
        }
    }

    fn write_multi(
        &mut self,
        head: HeadKind,
        level: u8,
        dir: bool,
        group: &[(ObjectId, u32)],
    ) -> Res<(ObjectId, u32), F> {
        let total: u32 = group.iter().map(|g| g.1).sum();
        let ids: Vec<ObjectId> = group.iter().map(|g| g.0).collect();
        let payload = encode_multi(level, dir, total, &ids);
        self.log.note(payload.capacity() + ids.capacity() * 8);
        let id = ObjectId::of(&mut self.hasher, IdTag::Multi, &[&payload]);
        self.write_if_new(head, RecordKind::Multi, ChunkCodec::Stored, id, &[&payload])?;
        Ok((id, total))
    }

    pub(crate) fn write_if_new(
        &mut self,
        head: HeadKind,
        kind: RecordKind,
        codec: ChunkCodec,
        id: ObjectId,
        parts: &[&[u8]],
    ) -> Res<(), F> {
        if self.log.index.contains(id) {
            stat!(self.stats.dedup_hits += 1);
            return Ok(());
        }
        self.log.append(head, kind, codec, id, parts)?;
        Ok(())
    }

    // ---- what a write will append (before it appends anything) ----------

    /// The stored chunks of `len` bytes; returns how many.
    fn stored_chunk_need(
        &self,
        head: HeadKind,
        len: usize,
        out: &mut Vec<(HeadKind, u32)>,
    ) -> usize {
        let mp = self.max_payload();
        let n = len.div_ceil(mp).max(1);
        for i in 0..n {
            let l = (len - i * mp).min(mp);
            out.push((head, HDR + l as u32));
        }
        n
    }

    /// The multi records over `leaves` leaves.
    pub(crate) fn multi_need(
        &self,
        head: HeadKind,
        leaves: usize,
        dir: bool,
        out: &mut Vec<(HeadKind, u32)>,
    ) {
        if leaves <= 1 && !dir {
            return;
        }
        let fanout = multi_fanout(self.max_payload());
        let mut n = leaves;
        loop {
            if n <= fanout {
                out.push((head, HDR + 7 + 8 * n as u32));
                return;
            }
            let groups = n.div_ceil(fanout);
            for g in 0..groups {
                let k = (n - g * fanout).min(fanout);
                out.push((head, HDR + 7 + 8 * k as u32));
            }
            n = groups;
        }
    }

    /// A per-call write also writes its directories and a root. The root is
    /// reserved here; each directory makes room for itself when it is
    /// written (`write_dir_bytes`), so a write that fits its content but not
    /// its directories ends in `NoSpace` after its content and before its
    /// root (the committed state untouched either way).
    fn implicit_need(&self, _path: &str, out: &mut Vec<(HeadKind, u32)>) {
        if self.txn != Txn::Implicit {
            return;
        }
        out.push((HeadKind::Hot, self.root_len()));
    }

    pub(crate) fn root_len(&self) -> u32 {
        HDR + 26 + 2 * self.log.sectors.retired.len() as u32
    }
}
