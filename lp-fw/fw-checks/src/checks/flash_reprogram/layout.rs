//! Where the payload's sectors are, and the order the work loop programs
//! the region in.

use super::{
    MAX_KEEP, MAX_SECTORS, MIN_REGION_SECTORS, REGION_FIRST, SECTOR_SIZE, STATE_SECTOR,
    TIMING_SECTOR,
};

/// The payload's sectors: the start of `lpfs` and how many of its sectors
/// it owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReprogramLayout {
    /// Absolute flash address of the first sector (the start of `lpfs`).
    pub base: u32,
    /// Sectors the payload owns from `base` (state, timing, region).
    pub sectors: u32,
}

impl ReprogramLayout {
    /// The layout at `base`, if `partition_len` bytes hold it.
    pub fn new(base: u32, partition_len: u32) -> Option<Self> {
        if base as usize % SECTOR_SIZE != 0 {
            return None;
        }
        let sectors = (partition_len / SECTOR_SIZE as u32).min(MAX_SECTORS);
        if sectors < REGION_FIRST + MIN_REGION_SECTORS {
            return None;
        }
        Some(Self { base, sectors })
    }

    /// Address of sector `s` (an index from `base`).
    pub fn sector_addr(&self, s: u32) -> u32 {
        debug_assert!(s < self.sectors);
        self.base + s * SECTOR_SIZE as u32
    }

    /// The sector index of absolute address `addr`.
    pub fn sector_of(&self, addr: u32) -> u32 {
        (addr - self.base) / SECTOR_SIZE as u32
    }

    pub fn state_addr(&self) -> u32 {
        self.sector_addr(STATE_SECTOR)
    }

    pub fn timing_addr(&self) -> u32 {
        self.sector_addr(TIMING_SECTOR)
    }

    /// Sectors in the region.
    pub fn region_sectors(&self) -> u32 {
        self.sectors - REGION_FIRST
    }
}

/// The region in the work loop's order: its sectors ascending, skipping the
/// ones a boot kept. A *logical* offset counts bytes in that order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgramOrder {
    pub layout: ReprogramLayout,
    /// Kept sector indices, ascending, `None` after the last.
    keep: [Option<u32>; MAX_KEEP],
}

impl ProgramOrder {
    /// The order with `keep` (sector indices inside the region) skipped.
    pub fn new(layout: ReprogramLayout, keep: &[u32]) -> Self {
        let mut k: [Option<u32>; MAX_KEEP] = [None; MAX_KEEP];
        let mut n = 0;
        for &s in keep {
            if n < MAX_KEEP
                && (REGION_FIRST..layout.sectors).contains(&s)
                && !k[..n].contains(&Some(s))
            {
                k[n] = Some(s);
                n += 1;
            }
        }
        k[..n].sort_unstable();
        Self { layout, keep: k }
    }

    /// The kept sectors.
    pub fn keep(&self) -> impl Iterator<Item = u32> + '_ {
        self.keep.iter().flatten().copied()
    }

    /// Sectors the loop programs.
    pub fn sectors(&self) -> u32 {
        self.layout.region_sectors() - self.keep().count() as u32
    }

    /// Bytes the loop programs.
    pub fn len(&self) -> u32 {
        self.sectors() * SECTOR_SIZE as u32
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The sector index of the loop's `k`-th sector.
    pub fn sector(&self, k: u32) -> u32 {
        let mut s = REGION_FIRST + k;
        for kept in self.keep() {
            if s >= kept {
                s += 1;
            }
        }
        s
    }

    /// The absolute address of logical offset `at`.
    pub fn addr(&self, at: u32) -> u32 {
        let ss = SECTOR_SIZE as u32;
        self.layout.sector_addr(self.sector(at / ss)) + at % ss
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn the_c6_partition_holds_state_timing_and_174_region_sectors() {
        let l = ReprogramLayout::new(0x35_0000, 0xB_0000).unwrap();
        assert_eq!(l.sectors, 176);
        assert_eq!(l.state_addr(), 0x35_0000);
        assert_eq!(l.timing_addr(), 0x35_1000);
        assert_eq!(l.region_sectors(), 174);
        assert!(ReprogramLayout::new(0x35_0100, 0xB_0000).is_none());
        assert!(ReprogramLayout::new(0x35_0000, 17 * 4096).is_none());
    }

    #[test]
    fn the_order_skips_kept_sectors() {
        let l = ReprogramLayout::new(0x35_0000, 0xB_0000).unwrap();
        let o = ProgramOrder::new(l, &[]);
        assert_eq!(o.sector(0), 2);
        assert_eq!(o.addr(4096 + 8), 0x35_3008);
        let o = ProgramOrder::new(l, &[5, 3, 5]);
        assert_eq!(o.keep().collect::<std::vec::Vec<_>>(), [3, 5]);
        assert_eq!(o.sectors(), 172);
        let order: std::vec::Vec<u32> = (0..6).map(|k| o.sector(k)).collect();
        assert_eq!(order, [2, 4, 6, 7, 8, 9]);
        assert_eq!(o.sector(o.sectors() - 1), 175);
        // Sectors outside the region are never kept.
        assert_eq!(ProgramOrder::new(l, &[0, 1, 176]).sectors(), 174);
    }
}
