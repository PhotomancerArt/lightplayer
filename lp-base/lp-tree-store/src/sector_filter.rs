//! Which sectors may hold an id: one small Bloom filter per sector, filled
//! by mount's first pass from the record headers it walks anyway, so the
//! closure walk (`mount_walk.rs`) scans only the sectors a level's ids may
//! be in. RAM only, rebuilt each mount, and only ever a hint: a "may hold"
//! is checked by reading the headers, and a "does not hold" is exact (every
//! record the walk could adopt was added).

use alloc::vec::Vec;

use crate::object_id::ObjectId;

/// Bits per sector.
const BITS: u32 = 128;
const WORDS: usize = (BITS / 32) as usize;

/// One filter per sector.
pub struct SectorFilter {
    bits: Vec<[u32; WORDS]>,
}

impl SectorFilter {
    pub fn new(sector_count: u32) -> Self {
        Self {
            bits: alloc::vec![[0; WORDS]; sector_count as usize],
        }
    }

    /// An id's two bits. Ids are hashes, so any of their bits will do.
    fn bits_of(id: ObjectId) -> [u32; 2] {
        [(id.0 as u32) % BITS, ((id.0 >> 32) as u32) % BITS]
    }

    pub fn add(&mut self, sector: u32, id: ObjectId) {
        let w = &mut self.bits[sector as usize];
        for b in Self::bits_of(id) {
            w[(b / 32) as usize] |= 1 << (b % 32);
        }
    }

    /// Whether `sector` may hold a record with `id` (false: it holds none).
    pub fn may_hold(&self, sector: u32, id: ObjectId) -> bool {
        let w = &self.bits[sector as usize];
        Self::bits_of(id)
            .iter()
            .all(|&b| w[(b / 32) as usize] & (1 << (b % 32)) != 0)
    }

    /// What the allocator holds for it.
    pub fn ram_bytes(&self) -> usize {
        self.bits.capacity() * size_of::<[u32; WORDS]>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holds_what_was_added_and_rarely_more() {
        let mut f = SectorFilter::new(4);
        let ids: Vec<ObjectId> = (1..=16u64)
            .map(|i| ObjectId(i.wrapping_mul(0x9E37_79B9_7F4A_7C15)))
            .collect();
        for &id in &ids {
            f.add(2, id);
        }
        assert!(ids.iter().all(|&id| f.may_hold(2, id)));
        assert!(ids.iter().all(|&id| !f.may_hold(1, id)));
        let others = (100..1100u64)
            .map(|i| ObjectId(i.wrapping_mul(0xD1B5_4A32_D192_ED03)))
            .filter(|&id| f.may_hold(2, id))
            .count();
        // 16 ids in 128 bits, two bits each: about 5 % of others.
        assert!(others < 120, "{others} of 1000");
    }
}
