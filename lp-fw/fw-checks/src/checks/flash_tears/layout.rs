//! Where the payload's sectors are.

use super::{JOURNAL_COPIES, LAYOUT_SECTORS, REGION_SECTORS, SECTOR_SIZE};

/// The payload's sectors, from the start of `lpfs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TearsLayout {
    /// Absolute flash address of the first sector (the start of `lpfs`).
    pub base: u32,
}

impl TearsLayout {
    /// The layout at `base`, if `partition_len` bytes hold it.
    pub fn new(base: u32, partition_len: u32) -> Option<Self> {
        if base as usize % SECTOR_SIZE != 0 {
            return None;
        }
        if (partition_len as usize) < LAYOUT_SECTORS as usize * SECTOR_SIZE {
            return None;
        }
        Some(Self { base })
    }

    /// Address of journal copy `copy`.
    pub fn journal_addr(&self, copy: u32) -> u32 {
        debug_assert!(copy < JOURNAL_COPIES);
        self.base + copy * SECTOR_SIZE as u32
    }

    /// Address of region sector `sector`.
    pub fn sector_addr(&self, sector: u32) -> u32 {
        debug_assert!(sector < REGION_SECTORS);
        self.base + (JOURNAL_COPIES + sector) * SECTOR_SIZE as u32
    }
}

/// The region sector cycle `cycle` erases and programs.
pub const fn sector_of(cycle: u32) -> u32 {
    cycle % REGION_SECTORS
}

/// The most recent cycle `<= latest` that wrote `sector`, if any did.
pub fn last_cycle_of(sector: u32, latest: u32) -> Option<u32> {
    let back = (latest + REGION_SECTORS - sector) % REGION_SECTORS;
    latest.checked_sub(back)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_puts_journals_first_then_the_region() {
        let l = TearsLayout::new(0x35_0000, 0xB_0000).unwrap();
        assert_eq!(l.journal_addr(0), 0x35_0000);
        assert_eq!(l.journal_addr(1), 0x35_1000);
        assert_eq!(l.sector_addr(0), 0x35_2000);
        assert_eq!(l.sector_addr(15), 0x35_2000 + 15 * 0x1000);
    }

    #[test]
    fn a_partition_too_small_or_unaligned_is_refused() {
        assert!(TearsLayout::new(0x35_0000, 17 * 4096).is_none());
        assert!(TearsLayout::new(0x35_0100, 0xB_0000).is_none());
    }

    #[test]
    fn last_cycle_of_walks_back_to_the_sector() {
        assert_eq!(last_cycle_of(3, 35), Some(35));
        assert_eq!(last_cycle_of(2, 35), Some(34));
        assert_eq!(last_cycle_of(4, 35), Some(20));
        assert_eq!(last_cycle_of(5, 2), None);
        assert_eq!(sector_of(35), 3);
    }
}
