//! Per-sector damage left by a torn operation: the weak-bit map.

use alloc::vec;
use alloc::vec::Vec;

/// What a power cut left in one sector beyond its cell values.
///
/// `weak` holds one mask byte per cell byte; a set bit is a *weak* bit, which
/// reads as a fresh random value on every read until the sector is erased in
/// full. `tainted` says a torn program or erase touched the sector since its
/// last complete erase (a 0→1 program there is a consequence of the tear, not
/// necessarily a store bug, so it is counted but never panics).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NorSectorState {
    pub weak: Option<Vec<u8>>,
    pub tainted: bool,
}

impl NorSectorState {
    pub fn is_pristine(&self) -> bool {
        self.weak.is_none() && !self.tainted
    }

    pub fn weak_mask(&mut self, sector_size: usize) -> &mut Vec<u8> {
        self.weak.get_or_insert_with(|| vec![0; sector_size])
    }

    pub fn weak_bits(&self) -> u64 {
        self.weak
            .as_ref()
            .map(|w| w.iter().map(|b| b.count_ones() as u64).sum())
            .unwrap_or(0)
    }
}
