//! The RAM index: id → where its one trusted copy lives, as a sorted array
//! of 12-byte entries (id u64, packed sector u16 | offset u16). Rebuilt at
//! mount from the chosen root's closure (`mount_walk.rs`); nothing on flash
//! describes it.
//!
//! What it holds (the dedup rule depends on it): after a mark, exactly the
//! live records; between marks, those plus every record written since. No
//! record it names is erased before the next mark prunes it, so every id it
//! holds has a complete closure on flash.

use alloc::vec::Vec;
use core::mem::size_of;

use crate::object_id::ObjectId;
use crate::vec_growth::grow_for_one;

/// Where a record's header is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordLoc {
    pub sector: u32,
    pub offset: u32,
}

impl RecordLoc {
    pub fn pack(self) -> u32 {
        self.sector << 16 | self.offset
    }

    pub fn unpack(v: u32) -> Self {
        RecordLoc {
            sector: v >> 16,
            offset: v & 0xFFFF,
        }
    }
}

#[derive(Clone, Copy)]
#[repr(C, packed(4))]
pub struct IndexEntry {
    id: u64,
    loc: u32,
}

/// Sorted by id; one entry per id.
#[derive(Default)]
pub struct RamIndex {
    entries: Vec<IndexEntry>,
}

impl RamIndex {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Position of `id`, or where it would go.
    pub fn find(&self, id: ObjectId) -> Result<usize, usize> {
        let (mut lo, mut hi) = (0, self.entries.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            let v = self.entries[mid].id;
            if v == id.0 {
                return Ok(mid);
            }
            if v < id.0 {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        Err(lo)
    }

    pub fn get(&self, id: ObjectId) -> Option<RecordLoc> {
        self.find(id).ok().map(|i| self.loc_at(i))
    }

    pub fn contains(&self, id: ObjectId) -> bool {
        self.find(id).is_ok()
    }

    pub fn id_at(&self, i: usize) -> ObjectId {
        ObjectId(self.entries[i].id)
    }

    pub fn loc_at(&self, i: usize) -> RecordLoc {
        RecordLoc::unpack(self.entries[i].loc)
    }

    /// Add or move.
    pub fn insert(&mut self, id: ObjectId, loc: RecordLoc) {
        let e = IndexEntry {
            id: id.0,
            loc: loc.pack(),
        };
        match self.find(id) {
            Ok(i) => self.entries[i] = e,
            Err(i) => {
                grow_for_one(&mut self.entries);
                self.entries.insert(i, e);
            }
        }
    }

    /// Keep the entries `keep(position)` says to.
    pub fn retain_positions(&mut self, mut keep: impl FnMut(usize) -> bool) {
        let mut i = 0;
        self.entries.retain(|_| {
            i += 1;
            keep(i - 1)
        });
    }

    /// Forget every record in `sector` (it is about to be erased).
    pub fn remove_sector(&mut self, sector: u32) {
        self.entries
            .retain(|e| RecordLoc::unpack(e.loc).sector != sector);
    }

    /// Positions of the entries in `sector`.
    pub fn positions_in(&self, sector: u32) -> impl Iterator<Item = usize> + '_ {
        (0..self.entries.len()).filter(move |&i| self.loc_at(i).sector == sector)
    }

    pub fn shrink(&mut self) {
        self.entries.shrink_to_fit();
    }

    /// What the allocator holds for it.
    pub fn ram_bytes(&self) -> usize {
        self.entries.capacity() * size_of::<IndexEntry>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(sector: u32, offset: u32) -> RecordLoc {
        RecordLoc { sector, offset }
    }

    #[test]
    fn insert_find_and_remove() {
        assert_eq!(size_of::<IndexEntry>(), 12);
        let mut ix = RamIndex::default();
        ix.insert(ObjectId(5), loc(0, 20));
        ix.insert(ObjectId(2), loc(1, 40));
        ix.insert(ObjectId(9), loc(0, 60));
        assert_eq!(ix.get(ObjectId(2)), Some(loc(1, 40)));
        ix.insert(ObjectId(2), loc(3, 20));
        assert_eq!(ix.get(ObjectId(2)), Some(loc(3, 20)));
        ix.remove_sector(0);
        assert_eq!(ix.len(), 1);
    }
}
