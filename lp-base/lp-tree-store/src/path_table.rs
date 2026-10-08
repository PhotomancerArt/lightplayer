//! The RAM path table: `path hash → (file id, size)` for every file, as a
//! sorted array of 20-byte rows — the store keeps no path strings. Reads
//! (`get`, `file_size`, `exists`) go through it; listings and writes walk
//! the `Dir` records on flash.
//!
//! **Collisions.** Two live paths with one 64-bit hash share a row marked
//! *collided* (file id 0): a lookup of that hash walks the directories
//! instead. Mount finds every such pair (it hashes every path); a write
//! finds one when a row exists for its hash but the walk says the path did
//! not exist. A collided row stays collided until the next mount, however
//! the paths change. A path that does not exist but hashes like one that
//! does reads as that file: the same 64-bit risk the content ids take
//! (spike U9), stated in FORMAT.md.

use alloc::vec::Vec;
use core::mem::size_of;

use crate::heap_sort::heap_sort_by;
use crate::object_id::ObjectId;
use crate::vec_growth::grow_for_one;

#[derive(Clone, Copy)]
#[repr(C, packed(4))]
struct PathRow {
    hash: u64,
    id: u64,
    size: u32,
}

/// What a row says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathSlot {
    /// No live path has this hash.
    Absent,
    File {
        id: ObjectId,
        size: u32,
    },
    /// Two or more live paths had this hash: walk.
    Collided,
}

#[derive(Default)]
pub struct PathTable {
    rows: Vec<PathRow>,
}

impl PathTable {
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    fn find(&self, hash: u64) -> Result<usize, usize> {
        let (mut lo, mut hi) = (0, self.rows.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            let v = self.rows[mid].hash;
            if v == hash {
                return Ok(mid);
            }
            if v < hash {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        Err(lo)
    }

    pub fn get(&self, hash: u64) -> PathSlot {
        match self.find(hash) {
            Err(_) => PathSlot::Absent,
            Ok(i) => {
                let r = self.rows[i];
                if r.id == 0 {
                    PathSlot::Collided
                } else {
                    PathSlot::File {
                        id: ObjectId(r.id),
                        size: r.size,
                    }
                }
            }
        }
    }

    /// Make the row for `hash` say `slot`.
    pub fn set(&mut self, hash: u64, slot: PathSlot) {
        let row = match slot {
            PathSlot::Absent => {
                if let Ok(i) = self.find(hash) {
                    self.rows.remove(i);
                }
                return;
            }
            PathSlot::File { id, size } => PathRow {
                hash,
                id: id.0,
                size,
            },
            PathSlot::Collided => PathRow {
                hash,
                id: 0,
                size: 0,
            },
        };
        match self.find(hash) {
            Ok(i) => self.rows[i] = row,
            Err(i) => {
                grow_for_one(&mut self.rows);
                self.rows.insert(i, row);
            }
        }
    }

    /// Mount: add a live file without keeping order; [`Self::finish_build`]
    /// after.
    pub fn push_unsorted(&mut self, hash: u64, id: ObjectId, size: u32) {
        self.rows.push(PathRow {
            hash,
            id: id.0,
            size,
        });
    }

    /// Mount: sort, and turn each run of equal hashes into one collided row.
    pub fn finish_build(&mut self) {
        heap_sort_by(&mut self.rows, |a, b| { a.hash } < { b.hash });
        let n = self.rows.len();
        let mut w = 0;
        let mut r = 0;
        while r < n {
            let h = self.rows[r].hash;
            let mut end = r + 1;
            while end < n && { self.rows[end].hash } == h {
                end += 1;
            }
            self.rows[w] = self.rows[r];
            if end - r > 1 {
                self.rows[w].id = 0;
                self.rows[w].size = 0;
            }
            w += 1;
            r = end;
        }
        self.rows.truncate(w);
        self.rows.shrink_to_fit();
    }

    pub fn ram_bytes(&self) -> usize {
        self.rows.capacity() * size_of::<PathRow>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_and_collisions() {
        assert_eq!(size_of::<PathRow>(), 20);
        let mut t = PathTable::default();
        t.push_unsorted(9, ObjectId(1), 10);
        t.push_unsorted(3, ObjectId(2), 20);
        t.push_unsorted(9, ObjectId(3), 30);
        t.finish_build();
        assert_eq!(t.len(), 2);
        assert_eq!(t.get(9), PathSlot::Collided);
        assert_eq!(
            t.get(3),
            PathSlot::File {
                id: ObjectId(2),
                size: 20
            }
        );
        t.set(3, PathSlot::Absent);
        assert_eq!(t.get(3), PathSlot::Absent);
        t.set(
            1,
            PathSlot::File {
                id: ObjectId(4),
                size: 1,
            },
        );
        assert_eq!(t.len(), 2);
    }
}
