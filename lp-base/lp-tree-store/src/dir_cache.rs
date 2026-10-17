//! The directories the last lookups read, kept in RAM: a project load reads
//! a hundred-odd files under the same few directories, and every lookup
//! walks the path from the root again (`tree_walk.rs`), reading and
//! CRC-checking `/`, `/projects`, `/projects/a`, `…/modules` each time — on
//! a c40-sized project about two thirds of what the boot reads before its
//! first light is those same directories again (the mount study, 2026-10-16).
//!
//! A directory node is content-addressed: its id is the hash of its bytes,
//! so an entry can never be wrong — not after a write, not after GC moved
//! the record, not after a power cut (RAM only; a mount starts empty). It
//! is still **dropped on every write** (each program and erase goes through
//! `RecordLog`, which clears it), for RAM, not correctness: the cache is for
//! the read-mostly stretch of a boot, and a write is where the store wants
//! its transient RAM. (So there is no mutant for it: keeping it over a write
//! changes no byte any read returns.) Bounded by `StoreConfig::dir_cache_max` bytes of
//! directory bytes, least recently used out first; a directory bigger than
//! that is never kept. 0 = no cache.

use alloc::vec::Vec;

use crate::object_id::ObjectId;

/// Directory node bytes by id, each with when it was last used.
#[derive(Default)]
pub struct DirCache {
    entries: Vec<(ObjectId, u32, Vec<u8>)>,
    bytes: usize,
    max: usize,
    /// The use counter: an entry's stamp is when it was last read or kept.
    now: u32,
}

impl DirCache {
    pub fn new(max_bytes: u32) -> Self {
        Self {
            max: max_bytes as usize,
            ..Self::default()
        }
    }

    /// A copy of node `id`'s bytes, if kept (it becomes the most recent).
    #[inline(never)]
    pub fn get(&mut self, id: ObjectId) -> Option<Vec<u8>> {
        self.now = self.now.wrapping_add(1);
        let e = self.entries.iter_mut().find(|e| e.0 == id)?;
        e.1 = self.now;
        Some(e.2.clone())
    }

    /// Keep a copy of node `id`'s `bytes` (unless they alone pass the
    /// bound), the least recently used out first to make room.
    #[inline(never)]
    pub fn put(&mut self, id: ObjectId, bytes: &[u8]) {
        if bytes.len() > self.max {
            return;
        }
        while self.bytes + bytes.len() > self.max {
            let Some(i) = (0..self.entries.len()).min_by_key(|&i| self.entries[i].1) else {
                break;
            };
            self.bytes -= self.entries.swap_remove(i).2.len();
        }
        self.bytes += bytes.len();
        self.entries.push((id, self.now, bytes.to_vec()));
    }

    /// Forget everything (a write).
    pub fn clear(&mut self) {
        if !self.entries.is_empty() {
            self.entries = Vec::new();
            self.bytes = 0;
        }
    }

    /// What the allocator holds for it.
    pub fn ram_bytes(&self) -> usize {
        self.bytes + self.entries.capacity() * size_of::<(ObjectId, u32, Vec<u8>)>()
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn keeps_the_most_recent_within_its_bound() {
        let mut c = DirCache::new(100);
        c.put(ObjectId(1), &[1; 40]);
        c.put(ObjectId(2), &[2; 40]);
        assert_eq!(c.get(ObjectId(1)), Some(vec![1u8; 40]));
        // 3 pushes out the least recent: 2.
        c.put(ObjectId(3), &[3; 40]);
        assert!(c.get(ObjectId(2)).is_none());
        assert!(c.get(ObjectId(1)).is_some() && c.get(ObjectId(3)).is_some());
        // Too big alone: not kept, nothing evicted.
        c.put(ObjectId(4), &[4; 101]);
        assert!(c.get(ObjectId(4)).is_none() && c.get(ObjectId(1)).is_some());
        c.clear();
        assert!(c.get(ObjectId(1)).is_none());
        let mut off = DirCache::new(0);
        off.put(ObjectId(5), &[5; 1]);
        assert!(off.get(ObjectId(5)).is_none());
    }
}
