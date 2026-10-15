//! Counters: what the stores asked of the flash.

use alloc::vec;
use alloc::vec::Vec;

/// Operation counters. Survive `power_cycle`; snapshot with `clone`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NorStats {
    /// Program pages + erases, over the sim's whole life.
    pub ops_total: u64,
    pub program_calls: u64,
    pub program_pages: u64,
    pub program_bytes: u64,
    pub erases_per_sector: Vec<u32>,
    pub read_calls: u64,
    pub read_bytes: u64,
    /// Programs that asked to turn a 0 bit back into 1 (real NOR cannot).
    pub violations_0_to_1: u64,
    /// Operations torn by a power cut (a cut that landed before an erase
    /// started counts: the op was cut).
    pub torn_ops: u64,
    /// Of those, sector erases that were torn (the rest are program pages
    /// and the erases cut before they started).
    pub torn_erases: u64,
    /// Cuts at an erase that landed before it started (the calibrated
    /// models' `cut_before_erase`): the sector was left as it was.
    pub erase_cuts_before_start: u64,
}

impl NorStats {
    pub fn new(sector_count: u32) -> Self {
        Self {
            erases_per_sector: vec![0; sector_count as usize],
            ..Default::default()
        }
    }

    pub fn erases_total(&self) -> u64 {
        self.erases_per_sector.iter().map(|&e| e as u64).sum()
    }
}
