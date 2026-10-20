//! Mount's whole id → location index: every record header mount's first
//! pass reads (`record_scan.rs`) is kept as one 12-byte entry, so the
//! closure walk (`mount_walk.rs`) finds each id it wants by one lookup
//! instead of scanning the headers of every sector that may hold it, one
//! scan per tree level. RAM only, built and dropped inside one mount;
//! nothing on flash describes it.
//!
//! **A hash table, not a sorted array.** Ids are hashes already, so an
//! open-addressed table (linear probing, the id's own bits as the slot) is
//! filled in one pass with no sort: on CX1 a heap sort of the same entries
//! cost more than the level scans it replaced (used/176: 70 → 165 ms,
//! 2026-10-16), the compares being most of a mount's CPU.
//!
//! **Which copy.** Two copies of an id (a GC copy and the original it has
//! not erased yet) are kept as the level walk finds them: the copy in the
//! sector later in sector-sequence order wins (FORMAT.md mount step 2), and
//! inside one sector the first. Pass 1 visits the sectors in that order, so
//! a copy from another sector replaces the one held, and a second copy in
//! the same sector does not.
//!
//! **Bounded.** It costs RAM for every record on flash, garbage included —
//! unbounded in the history — so its table is as big as
//! [`StoreConfig::mount_index_max`] allows, asked of the heap once (and
//! none at all if the heap cannot give it), and it is dropped where it
//! stands when it fills to fifteen sixteenths: the walk then scans by levels as
//! it did before (`mount_walk.rs`, the per-sector filters pass 1 keeps
//! beside it). Both readings adopt the same copies (`mount_walk.rs`'s
//! tests compare them).
//!
//! [`StoreConfig::mount_index_max`]: crate::StoreConfig::mount_index_max

use alloc::vec::Vec;

use crate::object_id::ObjectId;
use crate::ram_index::RecordLoc;

/// Fewest slots worth a table.
const MIN_SLOTS: usize = 64;

/// An id (two words, low first) and its packed location: 12 bytes at a
/// word's alignment. All zeros = an empty slot (no record has id 0; one
/// that did would drop the index).
#[derive(Clone, Copy)]
struct Entry([u32; 3]);

/// Every record mount's first pass read, by id.
pub struct MountIndex {
    slots: Vec<Entry>,
    len: usize,
}

impl MountIndex {
    /// The largest table of a power of two slots that fits `max_bytes`;
    /// `None` when fewer than [`MIN_SLOTS`] fit, or the heap cannot give it.
    pub fn new(max_bytes: u32) -> Option<Self> {
        let fit = max_bytes as usize / size_of::<Entry>();
        if fit < MIN_SLOTS {
            return None;
        }
        let n = 1usize << (usize::BITS - 1 - fit.leading_zeros());
        let mut slots = Vec::new();
        slots.try_reserve_exact(n).ok()?;
        slots.resize(n, Entry([0; 3]));
        Some(Self { slots, len: 0 })
    }

    /// Where `id` sits, or the empty slot it would take.
    fn probe(&self, id: u64) -> usize {
        let mask = self.slots.len() - 1;
        let key = [id as u32, (id >> 32) as u32];
        let mut i = (key[0] ^ key[1]) as usize & mask;
        loop {
            let e = self.slots[i].0;
            if e == [0; 3] || [e[0], e[1]] == key {
                return i;
            }
            i = (i + 1) & mask;
        }
    }

    /// Add a record pass 1 read (sectors in sector-sequence order); `false`
    /// when the table is full (the index is then useless and must be
    /// dropped).
    pub fn push(&mut self, id: ObjectId, loc: RecordLoc) -> bool {
        if id.0 == 0 || self.len >= self.slots.len() - self.slots.len() / 16 {
            return false;
        }
        let i = self.probe(id.0);
        let e = &mut self.slots[i];
        if e.0 == [0; 3] {
            self.len += 1;
        } else if RecordLoc::unpack(e.0[2]).sector == loc.sector {
            // The first copy in a sector is the one kept.
            return true;
        }
        *e = Entry([id.0 as u32, (id.0 >> 32) as u32, loc.pack()]);
        true
    }

    /// Where the adopted copy of `id` is.
    pub fn get(&self, id: ObjectId) -> Option<RecordLoc> {
        if id.0 == 0 {
            return None;
        }
        let e = self.slots[self.probe(id.0)].0;
        (e != [0; 3]).then(|| RecordLoc::unpack(e[2]))
    }

    /// What the allocator holds for it.
    pub fn ram_bytes(&self) -> usize {
        self.slots.capacity() * size_of::<Entry>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(sector: u32, offset: u32) -> RecordLoc {
        RecordLoc { sector, offset }
    }

    #[test]
    fn a_later_sector_then_the_first_offset_wins() {
        assert_eq!(size_of::<Entry>(), 12);
        let mut ix = MountIndex::new(64 * 1024).unwrap();
        // Pushed as pass 1 visits them: sector 1, then 3, then 2.
        let adds: [(u64, u32, u32); 6] = [
            (7, 1, 40),
            (9, 1, 24),
            (5, 1, 8),
            (7, 3, 20),
            (7, 3, 90),
            (9, 2, 24),
        ];
        for (id, s, off) in adds {
            assert!(ix.push(ObjectId(id), loc(s, off)));
        }
        // Many more, colliding in the low bits.
        for i in 0..1000u64 {
            assert!(ix.push(ObjectId((i + 1) << 32 | 7), loc(0, i as u32)));
        }
        assert_eq!(ix.get(ObjectId(7)), Some(loc(3, 20)));
        assert_eq!(ix.get(ObjectId(9)), Some(loc(2, 24)));
        assert_eq!(ix.get(ObjectId(5)), Some(loc(1, 8)));
        assert_eq!(ix.get(ObjectId(6)), None);
        assert_eq!(ix.get(ObjectId(0)), None);
        assert_eq!(ix.get(ObjectId(2 << 32 | 7)), Some(loc(0, 1)));
    }

    #[test]
    fn refuses_at_fifteen_sixteenths_and_below_its_floor() {
        let mut ix = MountIndex::new(12 * 300).unwrap();
        // 300 fit: a table of 256, full at 240.
        assert_eq!(ix.ram_bytes(), 256 * 12);
        for i in 1..=240u64 {
            assert!(ix.push(ObjectId(i.wrapping_mul(0x9E37_79B9_7F4A_7C15)), loc(0, 0)));
        }
        assert!(!ix.push(ObjectId(1), loc(0, 0)));
        assert!(
            !MountIndex::new(12 * 300)
                .unwrap()
                .push(ObjectId(0), loc(0, 0))
        );
        assert!(MountIndex::new(12 * 63).is_none());
    }
}
