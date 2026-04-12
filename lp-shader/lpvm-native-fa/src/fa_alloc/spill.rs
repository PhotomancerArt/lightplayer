//! Spill slot allocation and tracking.
//!
//! Assigns frame-pointer-relative spill slots on demand. Uses u8 slot
//! indices — sufficient for shaders (< 256 spills).

use crate::vinst::VReg;
use alloc::vec::Vec;

/// Spill slot allocator.
pub struct SpillAlloc {
    /// Spill slot for each vreg index. None = not spilled.
    slots: Vec<Option<u8>>,
    /// Next available slot index.
    next_slot: u8,
}

impl SpillAlloc {
    pub fn new(num_vregs: usize) -> Self {
        Self {
            slots: vec![None; num_vregs],
            next_slot: 0,
        }
    }

    /// Get existing spill slot or assign a new one.
    pub fn get_or_assign(&mut self, vreg: VReg) -> u8 {
        let idx = vreg.0 as usize;
        if let Some(slot) = self.slots[idx] {
            slot
        } else {
            let slot = self.next_slot;
            self.slots[idx] = Some(slot);
            self.next_slot += 1;
            slot
        }
    }

    /// Check if vreg has a spill slot.
    pub fn has_slot(&self, vreg: VReg) -> Option<u8> {
        self.slots[vreg.0 as usize]
    }

    /// Total spill slots used.
    pub fn total_slots(&self) -> u32 {
        self.next_slot as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spill_assign_and_retrieve() {
        let mut s = SpillAlloc::new(4);
        assert_eq!(s.has_slot(VReg(0)), None);
        assert_eq!(s.get_or_assign(VReg(0)), 0);
        assert_eq!(s.get_or_assign(VReg(0)), 0); // same slot
        assert_eq!(s.get_or_assign(VReg(2)), 1);
        assert_eq!(s.total_slots(), 2);
    }

    #[test]
    fn spill_multiple_vregs() {
        let mut s = SpillAlloc::new(100);
        for i in 0u16..50 {
            let slot = s.get_or_assign(VReg(i));
            assert_eq!(slot as u16, i);
        }
        assert_eq!(s.total_slots(), 50);

        // Re-querying returns same slots
        for i in 0u16..50 {
            assert_eq!(s.get_or_assign(VReg(i)), i as u8);
        }
    }
}
