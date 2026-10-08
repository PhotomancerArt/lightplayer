//! What the store knows about each sector, in RAM only.

use alloc::vec;
use alloc::vec::Vec;
use core::mem::size_of;

use crate::sector_header::SectorHeader;

/// A sector's use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectorUse {
    /// No valid header: whatever it reads, it must be erased before use.
    NeedsErase,
    /// Erased this session, header not yet programmed: ready to open.
    Erased,
    /// Valid header; records occupy `[header .. end)`. A closed sector (a
    /// record failed to check) has `end` = the sector size.
    Written { header: SectorHeader, end: u32 },
}

/// Per-sector state, erase counts and live bytes (from the last mark).
#[derive(Clone, Debug)]
pub struct SectorTable {
    pub uses: Vec<SectorUse>,
    pub erase_counts: Vec<u32>,
    pub live_bytes: Vec<u32>,
}

impl SectorTable {
    pub fn new(sector_count: u32) -> Self {
        let n = sector_count as usize;
        Self {
            uses: vec![SectorUse::NeedsErase; n],
            erase_counts: vec![0; n],
            live_bytes: vec![0; n],
        }
    }

    pub fn ram_bytes(&self) -> usize {
        self.uses.len() * (size_of::<SectorUse>() + 2 * size_of::<u32>())
    }
}
