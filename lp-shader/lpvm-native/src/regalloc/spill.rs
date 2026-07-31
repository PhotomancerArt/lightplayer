//! Spill slot allocation and tracking.
//!
//! Assigns frame-pointer-relative spill slots on demand. Uses u8 slot
//! indices — sufficient for shaders (< 256 spills).
//!
//! Slots are **class-tagged, not class-partitioned**: there is one index space,
//! and each slot records the [`RegClass`] of the value it holds. A word is a
//! word, so an integer slot and a float slot are the same four bytes in the
//! frame and splitting the index space would buy nothing but a bigger frame.
//! What the tag buys is the check: a reload out of a slot must land in a
//! register of the slot's own class, which is the one way a two-class allocator
//! can quietly produce a bit-reinterpretation instead of a value.

use crate::abi::RegClass;
use crate::vinst::VReg;
use alloc::vec::Vec;

/// Spill slot allocator.
pub struct SpillAlloc {
    /// Spill slot for each vreg index. None = not spilled.
    slots: Vec<Option<u8>>,
    /// Register class of each assigned slot, indexed by slot number.
    slot_classes: Vec<RegClass>,
    /// Next available slot index.
    next_slot: u8,
}

impl SpillAlloc {
    pub fn new(num_vregs: usize) -> Self {
        Self {
            slots: vec![None; num_vregs],
            slot_classes: Vec::new(),
            next_slot: 0,
        }
    }

    /// Get existing spill slot or assign a new one holding a `class` value.
    pub fn get_or_assign(&mut self, vreg: VReg, class: RegClass) -> u8 {
        let idx = vreg.0 as usize;
        if let Some(slot) = self.slots[idx] {
            slot
        } else {
            let slot = self.next_slot;
            self.slots[idx] = Some(slot);
            self.slot_classes.push(class);
            self.next_slot += 1;
            slot
        }
    }

    /// Check if vreg has a spill slot.
    pub fn has_slot(&self, vreg: VReg) -> Option<u8> {
        self.slots[vreg.0 as usize]
    }

    /// Register class of an assigned slot, or `None` if the slot was never
    /// assigned.
    pub fn slot_class(&self, slot: u8) -> Option<RegClass> {
        self.slot_classes.get(slot as usize).copied()
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
        assert_eq!(s.get_or_assign(VReg(0), RegClass::Int), 0);
        assert_eq!(s.get_or_assign(VReg(0), RegClass::Int), 0); // same slot
        assert_eq!(s.get_or_assign(VReg(2), RegClass::Int), 1);
        assert_eq!(s.total_slots(), 2);
    }

    #[test]
    fn spill_multiple_vregs() {
        let mut s = SpillAlloc::new(100);
        for i in 0u16..50 {
            let slot = s.get_or_assign(VReg(i), RegClass::Int);
            assert_eq!(slot as u16, i);
        }
        assert_eq!(s.total_slots(), 50);

        // Re-querying returns same slots
        for i in 0u16..50 {
            assert_eq!(s.get_or_assign(VReg(i), RegClass::Int), i as u8);
        }
    }

    /// Both classes draw from one index space — the numbering must not change
    /// when a float value is spilled between two integer ones.
    #[test]
    fn slots_record_their_class_in_one_index_space() {
        let mut s = SpillAlloc::new(4);
        assert_eq!(s.get_or_assign(VReg(0), RegClass::Int), 0);
        assert_eq!(s.get_or_assign(VReg(1), RegClass::Float), 1);
        assert_eq!(s.get_or_assign(VReg(2), RegClass::Int), 2);
        assert_eq!(s.slot_class(0), Some(RegClass::Int));
        assert_eq!(s.slot_class(1), Some(RegClass::Float));
        assert_eq!(s.slot_class(2), Some(RegClass::Int));
        assert_eq!(s.slot_class(3), None);
        assert_eq!(s.total_slots(), 3);
    }
}
