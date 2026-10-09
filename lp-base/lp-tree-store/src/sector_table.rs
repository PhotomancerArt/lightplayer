//! What the store knows about each sector, in RAM only: four compact
//! columns (12 B per sector) and the retired list.

use alloc::vec;
use alloc::vec::Vec;

/// `end` value: no valid header — whatever it reads, it must be erased.
pub const NEEDS_ERASE: u16 = 0;
/// `end` value: erased (and verified) this session, header not programmed.
pub const ERASED: u16 = 1;

/// Per-sector state.
pub struct SectorTable {
    /// `NEEDS_ERASE`, `ERASED`, or the offset past the last record of a
    /// written sector (≥ the sector header; the sector size when closed).
    pub end: Vec<u16>,
    /// Live bytes: exact after a mark, an upper bound between marks (writes
    /// add, nothing subtracts).
    pub live: Vec<u16>,
    /// From the header (0 when the header was unreadable).
    pub erase_count: Vec<u32>,
    /// The header's sequence (GC age).
    pub seq: Vec<u32>,
    /// Retired sectors, ascending: never opened, erased or collected again.
    pub retired: Vec<u16>,
}

impl SectorTable {
    pub fn new(sector_count: u32) -> Self {
        let n = sector_count as usize;
        Self {
            end: vec![NEEDS_ERASE; n],
            live: vec![0; n],
            erase_count: vec![0; n],
            seq: vec![0; n],
            retired: Vec::new(),
        }
    }

    pub fn is_written(&self, s: u32) -> bool {
        self.end[s as usize] > ERASED
    }

    pub fn is_retired(&self, s: u32) -> bool {
        self.retired.binary_search(&(s as u16)).is_ok()
    }

    pub fn retire(&mut self, s: u32) {
        if let Err(i) = self.retired.binary_search(&(s as u16)) {
            self.retired.insert(i, s as u16);
        }
    }

    pub fn add_live(&mut self, s: u32, n: u32) {
        let v = &mut self.live[s as usize];
        *v = v.saturating_add(n as u16);
    }

    #[cfg(feature = "stats")]
    pub fn ram_bytes(&self) -> usize {
        self.end.capacity() * core::mem::size_of::<u16>()
            + self.live.capacity() * core::mem::size_of::<u16>()
            + self.erase_count.capacity() * core::mem::size_of::<u32>()
            + self.seq.capacity() * core::mem::size_of::<u32>()
            + self.retired.capacity() * core::mem::size_of::<u16>()
    }
}
