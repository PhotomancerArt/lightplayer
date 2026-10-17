//! Mount's whole id → location index: every record header mount's first
//! pass reads (`record_scan.rs`) is kept as one 12-byte entry, so the
//! closure walk (`mount_walk.rs`) finds each id it wants by a binary search
//! instead of scanning the headers of every sector that may hold it, one
//! scan per tree level. RAM only, built and dropped inside one mount;
//! nothing on flash describes it.
//!
//! **Which copy.** Two copies of an id (a GC copy and the original it has
//! not erased yet) are ordered as the level walk finds them: the copy in the
//! sector later in sector-sequence order wins (FORMAT.md mount step 2), and
//! inside one sector the first. Entries are sorted by id, then by that
//! preference, so the first entry of an id's run is the one mount adopts.
//!
//! **Bounded.** It costs RAM for every record on flash, garbage included —
//! unbounded in the history. Past [`StoreConfig::mount_index_max`] bytes it
//! is dropped where it stands and the walk scans by levels as it did before
//! (`mount_walk.rs`, the per-sector filters pass 1 keeps beside it). Both
//! readings adopt the same copies (`mount_walk.rs`'s tests compare them).
//! Entries live in fixed blocks, never one growing array: no copy on growth,
//! so the peak is the cap, and no single large block is asked of the heap.
//!
//! [`StoreConfig::mount_index_max`]: crate::StoreConfig::mount_index_max

use alloc::vec::Vec;

use crate::heap_sort::heap_sort_positions;
use crate::object_id::ObjectId;
use crate::ram_index::RecordLoc;

/// Entries per block (3 KiB).
const BLOCK: usize = 256;

/// An id (two words, low first) and its packed location: 12 bytes at a
/// word's alignment (no packed struct).
#[derive(Clone, Copy)]
struct Entry([u32; 3]);

impl Entry {
    fn new(id: ObjectId, loc: RecordLoc) -> Self {
        Self([id.0 as u32, (id.0 >> 32) as u32, loc.pack()])
    }

    fn id(self) -> u64 {
        u64::from(self.0[0]) | u64::from(self.0[1]) << 32
    }

    /// Its location, as pushed (before [`MountIndex::sort`]).
    fn loc(self) -> RecordLoc {
        RecordLoc::unpack(self.0[2])
    }
}

/// Every record mount's first pass read, by id.
pub struct MountIndex {
    blocks: Vec<Vec<Entry>>,
    len: usize,
    /// Most entries allowed.
    max: usize,
}

impl MountIndex {
    /// An index that holds at most `max_bytes` of entries (`None`: none at
    /// all fit, so there is no index).
    pub fn new(max_bytes: u32) -> Option<Self> {
        let max = max_bytes as usize / size_of::<Entry>();
        (max > 0).then(|| Self {
            blocks: Vec::new(),
            len: 0,
            max,
        })
    }

    /// Add a record pass 1 read; `false` when it does not fit (the index is
    /// then useless and must be dropped).
    pub fn push(&mut self, id: ObjectId, loc: RecordLoc) -> bool {
        if self.len == self.max {
            return false;
        }
        if self.len.is_multiple_of(BLOCK) {
            self.blocks.push(alloc::vec![Entry([0; 3]); BLOCK]);
        }
        let (b, i) = (self.len / BLOCK, self.len % BLOCK);
        self.blocks[b][i] = Entry::new(id, loc);
        self.len += 1;
        true
    }

    fn at(&self, i: usize) -> Entry {
        self.blocks[i / BLOCK][i % BLOCK]
    }

    /// Sort for lookups. `rank[s]`: sector `s`'s place in sector-sequence
    /// order (later = newer). Each entry's sector becomes its rank, counted
    /// from the newest (so a plain compare of `(id, place)` puts the copy in
    /// the newest sector, and then the one at the lowest offset, first); a
    /// lookup turns it back.
    pub fn sort(&mut self, rank: &[u16]) {
        for b in &mut self.blocks {
            for e in b.iter_mut() {
                let loc = e.loc();
                if let Some(&r) = rank.get(loc.sector as usize) {
                    e.0[2] = u32::from(u16::MAX - r) << 16 | loc.offset;
                }
            }
        }
        let blocks = &mut self.blocks;
        heap_sort_positions(self.len, &mut |i, j, swap| {
            let (a, b) = (blocks[i / BLOCK][i % BLOCK], blocks[j / BLOCK][j % BLOCK]);
            if swap {
                blocks[i / BLOCK][i % BLOCK] = b;
                blocks[j / BLOCK][j % BLOCK] = a;
                false
            } else {
                (a.0[1], a.0[0], a.0[2]) < (b.0[1], b.0[0], b.0[2])
            }
        });
    }

    /// Where the adopted copy of `id` is (after [`sort`](Self::sort));
    /// `sectors`: the trusted sectors in sector-sequence order (the ranks).
    pub fn get(&self, id: ObjectId, sectors: &[u16]) -> Option<RecordLoc> {
        let (mut lo, mut hi) = (0, self.len);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.at(mid).id() < id.0 {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo == self.len || self.at(lo).id() != id.0 {
            return None;
        }
        let place = self.at(lo).0[2];
        let rank = usize::from(u16::MAX - (place >> 16) as u16);
        Some(RecordLoc {
            sector: u32::from(*sectors.get(rank)?),
            offset: place & 0xFFFF,
        })
    }

    /// What the allocator holds for it.
    pub fn ram_bytes(&self) -> usize {
        self.blocks.len() * BLOCK * size_of::<Entry>()
            + self.blocks.capacity() * size_of::<Vec<Entry>>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(sector: u32, offset: u32) -> RecordLoc {
        RecordLoc { sector, offset }
    }

    #[test]
    fn the_newest_sector_then_the_first_offset_wins() {
        assert_eq!(size_of::<Entry>(), 12);
        let mut ix = MountIndex::new(64 * 1024).unwrap();
        // Sector 2 is the newest, then 3, then 1, then 0.
        let rank: [u16; 4] = [0, 1, 3, 2];
        let adds: [(u64, u32, u32); 6] = [
            (7, 1, 40),
            (7, 3, 90),
            (7, 3, 20),
            (9, 2, 24),
            (9, 1, 24),
            (5, 1, 8),
        ];
        for (id, s, off) in adds {
            assert!(ix.push(ObjectId(id), loc(s, off)));
        }
        // Many more, across blocks: ids 1000..2000, offset = the id's step.
        for i in 0..1000u32 {
            let id = 1000 + u64::from(i * 7 % 1000);
            assert!(ix.push(ObjectId(id), loc(0, i)));
        }
        ix.sort(&rank);
        let valid: [u16; 4] = [0, 1, 3, 2];
        assert_eq!(ix.get(ObjectId(7), &valid), Some(loc(3, 20)));
        assert_eq!(ix.get(ObjectId(9), &valid), Some(loc(2, 24)));
        assert_eq!(ix.get(ObjectId(5), &valid), Some(loc(1, 8)));
        assert_eq!(ix.get(ObjectId(6), &valid), None);
        assert_eq!(ix.get(ObjectId(2000), &valid), None);
        assert_eq!(ix.get(ObjectId(1007), &valid), Some(loc(0, 1)));
    }

    #[test]
    fn refuses_past_its_cap() {
        let mut ix = MountIndex::new(12 * 300).unwrap();
        for i in 0..300u64 {
            assert!(ix.push(ObjectId(i), loc(0, 0)));
        }
        assert!(!ix.push(ObjectId(300), loc(0, 0)));
        assert!(MountIndex::new(11).is_none());
        assert!(ix.ram_bytes() >= 2 * BLOCK * 12);
    }
}
