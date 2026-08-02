//! Spill slot allocation and tracking.
//!
//! Assigns frame-pointer-relative spill slots on demand. Slot indices are `u8`,
//! and [`SpillAlloc::MAX_SLOTS`] is a hard ceiling the allocator reports rather
//! than wraps past — see [`SpillAlloc::get_or_assign`].
//!
//! Slots are **class-tagged, not class-partitioned**: there is one index space,
//! and each slot records the [`RegClass`] of the value it holds. A word is a
//! word, so an integer slot and a float slot are the same four bytes in the
//! frame and splitting the index space would buy nothing but a bigger frame.
//!
//! What the tag buys is [`SpillAlloc::get_or_assign`]'s consistency check. The
//! walk derives an operand's class at roughly a dozen call sites — off the
//! defining instruction, off the register a value was evicted from, off the
//! pool home it is being parked out of — and every one of them must agree for a
//! given vreg. If two of them disagree, a value is spilled as one class and
//! reloaded as the other, which is not a crash or a bad address but a silent
//! bit reinterpretation. The slot is where those independent derivations meet,
//! so it is the cheapest place to notice.

use crate::abi::RegClass;
use crate::regalloc::AllocError;
use crate::vinst::VReg;
use alloc::vec::Vec;

/// Spill slot allocator.
pub struct SpillAlloc {
    /// Spill slot for each vreg index. None = not spilled.
    slots: Vec<Option<u8>>,
    /// Register class of each assigned slot, indexed by slot number.
    slot_classes: Vec<RegClass>,
    /// Next available slot index. Wider than the `u8` it hands out so that
    /// "exhausted" is a representable state rather than a wrap back to slot 0.
    next_slot: u16,
}

impl SpillAlloc {
    /// Spill slots one function may have.
    ///
    /// The index space is what [`Alloc::Stack`](crate::regalloc::Alloc::Stack)
    /// can name, and it is a `u8` on purpose. Widening it is not a one-line
    /// change:
    ///
    /// - `Alloc` is pinned at two bytes (see the `size_of` assert in
    ///   [`crate::regalloc`]); it is one entry per operand of every instruction
    ///   in the function, built in the JIT's heap on the device.
    /// - Xtensa's `l32i`/`lsi` reach 1020 bytes from the base register, which
    ///   is exactly 255 word-sized slots. Past that the emitters take an
    ///   address-materialisation fallback — correct, but two extra instructions
    ///   on every spill access.
    /// - rv32's spill accesses have no such fallback: `encode_lw`/`encode_sw`
    ///   only `debug_assert` the signed-12-bit field, so a frame that outgrows
    ///   ±2048 truncates the offset in a release build.
    ///
    /// So the ceiling stays, and overrunning it is reported.
    pub const MAX_SLOTS: u16 = 256;

    pub fn new(num_vregs: usize) -> Self {
        Self {
            slots: vec![None; num_vregs],
            slot_classes: Vec::new(),
            next_slot: 0,
        }
    }

    /// Get existing spill slot or assign a new one holding a `class` value.
    ///
    /// Returns [`AllocError::TooManySpillSlots`] once [`Self::MAX_SLOTS`] are
    /// handed out. That is a real refusal to compile the function, not a
    /// nicety: the counter used to be the `u8` it hands out, so the 257th slot
    /// wrapped to 0 — a panic where overflow checks are on, and where they are
    /// not (the firmware's release profile) a fresh vreg quietly aliasing slot
    /// 0 with whatever already lived there. Two live values sharing four bytes
    /// of frame is a miscompile that reaches the strip, and `fw-esp32v3`
    /// compiles authored GLSL on-device, so the input is a user's shader.
    ///
    /// Asserts (debug builds) that repeat requests for the same vreg agree on
    /// its class — see the module doc. A `debug_assert` rather than a hard one
    /// because this runs inside the on-device compiler, where a panic takes the
    /// firmware down; host tests and the filetest corpus are where the check
    /// has to bite, and they run it on every spill.
    pub fn get_or_assign(&mut self, vreg: VReg, class: RegClass) -> Result<u8, AllocError> {
        let idx = vreg.0 as usize;
        if let Some(slot) = self.slots[idx] {
            debug_assert_eq!(
                self.slot_classes[slot as usize], class,
                "vreg {} was assigned a {:?} spill slot and is now being asked for a {class:?} one",
                vreg.0, self.slot_classes[slot as usize]
            );
            Ok(slot)
        } else {
            if self.next_slot >= Self::MAX_SLOTS {
                return Err(AllocError::TooManySpillSlots {
                    max: u32::from(Self::MAX_SLOTS),
                });
            }
            let slot = self.next_slot as u8;
            self.slots[idx] = Some(slot);
            self.slot_classes.push(class);
            self.next_slot += 1;
            Ok(slot)
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
        u32::from(self.next_slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every assignment in these tests is expected to succeed; the exhaustion
    /// path has tests of its own below.
    fn assign(s: &mut SpillAlloc, vreg: VReg, class: RegClass) -> u8 {
        s.get_or_assign(vreg, class)
            .expect("slot space not exhausted")
    }

    #[test]
    fn spill_assign_and_retrieve() {
        let mut s = SpillAlloc::new(4);
        assert_eq!(s.has_slot(VReg(0)), None);
        assert_eq!(assign(&mut s, VReg(0), RegClass::Int), 0);
        assert_eq!(assign(&mut s, VReg(0), RegClass::Int), 0); // same slot
        assert_eq!(assign(&mut s, VReg(2), RegClass::Int), 1);
        assert_eq!(s.total_slots(), 2);
    }

    #[test]
    fn spill_multiple_vregs() {
        let mut s = SpillAlloc::new(100);
        for i in 0u16..50 {
            let slot = assign(&mut s, VReg(i), RegClass::Int);
            assert_eq!(slot as u16, i);
        }
        assert_eq!(s.total_slots(), 50);

        // Re-querying returns same slots
        for i in 0u16..50 {
            assert_eq!(assign(&mut s, VReg(i), RegClass::Int), i as u8);
        }
    }

    /// The whole index space must be usable — an off-by-one that stopped at 255
    /// would be a silent capacity regression, since nothing else reports the
    /// ceiling — and the slot after it must be an error rather than a wrap to 0.
    #[test]
    fn the_last_slot_is_assignable_and_the_next_one_is_an_error() {
        let n = SpillAlloc::MAX_SLOTS;
        let mut s = SpillAlloc::new(n as usize + 1);
        for i in 0..n {
            assert_eq!(assign(&mut s, VReg(i), RegClass::Int) as u16, i);
        }
        assert_eq!(s.total_slots(), u32::from(n));
        assert_eq!(s.slot_class(255), Some(RegClass::Int));

        assert_eq!(
            s.get_or_assign(VReg(n), RegClass::Int),
            Err(AllocError::TooManySpillSlots {
                max: u32::from(SpillAlloc::MAX_SLOTS)
            }),
        );
        // The rejection must not have consumed anything, and must not have
        // recorded a class for a slot that was never handed out.
        assert_eq!(s.total_slots(), u32::from(n));
        assert_eq!(s.has_slot(VReg(n)), None);
    }

    /// A vreg that already owns a slot keeps working after the space fills —
    /// the failure is "no *new* slot", not "no slots".
    #[test]
    fn an_already_assigned_vreg_still_resolves_when_the_space_is_full() {
        let mut s = SpillAlloc::new(SpillAlloc::MAX_SLOTS as usize + 1);
        for i in 0..SpillAlloc::MAX_SLOTS {
            assign(&mut s, VReg(i), RegClass::Int);
        }
        assert_eq!(s.get_or_assign(VReg(7), RegClass::Int), Ok(7));
    }

    /// Both classes draw from one index space — the numbering must not change
    /// when a float value is spilled between two integer ones.
    #[test]
    fn slots_record_their_class_in_one_index_space() {
        let mut s = SpillAlloc::new(4);
        assert_eq!(assign(&mut s, VReg(0), RegClass::Int), 0);
        assert_eq!(assign(&mut s, VReg(1), RegClass::Float), 1);
        assert_eq!(assign(&mut s, VReg(2), RegClass::Int), 2);
        assert_eq!(s.slot_class(0), Some(RegClass::Int));
        assert_eq!(s.slot_class(1), Some(RegClass::Float));
        assert_eq!(s.slot_class(2), Some(RegClass::Int));
        assert_eq!(s.slot_class(3), None);
        assert_eq!(s.total_slots(), 3);
    }

    /// The check the tag exists for: two call sites deriving different classes
    /// for one vreg means a value gets spilled as one and reloaded as the
    /// other. Debug-only, so the test asserts the panic directly.
    #[test]
    #[should_panic(expected = "spill slot and is now being asked for")]
    #[cfg(debug_assertions)]
    fn reassigning_a_slot_with_a_different_class_is_a_bug() {
        let mut s = SpillAlloc::new(2);
        let _ = s.get_or_assign(VReg(0), RegClass::Int);
        let _ = s.get_or_assign(VReg(0), RegClass::Float);
    }
}
