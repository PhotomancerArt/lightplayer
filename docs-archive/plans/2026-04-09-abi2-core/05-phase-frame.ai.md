# Phase 5: Frame Layout

## Scope

Implement frame layout computation in `abi2/frame.rs`. `FrameLayout` is computed after regalloc runs, using the ABI constraints and regalloc results.

## Code Organization

- `SlotKind` enum at top
- `FrameLayout` struct definition
- `FrameLayout::compute()` implementation
- Helper methods (offset calculations)
- Tests at end

## Implementation Details

### SlotKind

```rust
/// Classification of a stack slot
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    /// Spill slot assigned by register allocator
    Spill { index: u32 },
    /// LPIR semantic slot (arrays, structs, etc.)
    Lpir { slot_id: u32, size: u32 },
}
```

### FrameLayout

```rust
/// Physical stack frame layout.
/// 
/// Layout (high to low addresses, i.e., growth direction):
/// ```
/// High addresses (toward 0x80000000)
/// |
/// |  [incoming args - caller's frame]
/// |  -------------------------------  <-- Entry SP
/// |  | saved RA          |  -4      |  (if non-leaf)
/// |  | saved S0 (FP)     |  -8      |
/// |  | saved callee regs |  -12...  |
/// |  -------------------------------
/// |  | spill slot 0      |  -N      |
/// |  | spill slot 1      |  -N-4    |  (spill_count slots)
/// |  ...
/// |  -------------------------------
/// |  | LPIR slot 0       |  -M      |  (semantic slots)
/// |  | LPIR slot 1       |  -M-size |
/// |  ...
/// v  -------------------------------  <-- SP after prologue (16-byte aligned)
/// Low addresses (growing down)
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameLayout {
    /// Total frame size in bytes (16-byte aligned)
    pub total_size: u32,
    
    /// Offset from SP to saved RA (if Some)
    pub ra_offset: Option<i32>,
    
    /// Offset from SP to saved S0/frame pointer (if Some)
    pub fp_offset: Option<i32>,
    
    /// Callee-saved registers to save: (reg, offset from SP)
    pub callee_saves: Vec<(PReg, i32)>,
    
    /// Base offset for spill slot 0 (from SP)
    /// Spill slot N is at spill_base - (N * 4)
    pub spill_base: i32,
    
    /// Base offset for LPIR slot area
    /// Individual slots computed from lpir_slots
    pub lpir_base: i32,
    
    /// Number of spill slots
    pub spill_count: u32,
    
    /// LPIR slot offsets: (slot_id, offset from SP)
    pub lpir_slots: Vec<(u32, i32)>,
}

impl FrameLayout {
    /// Compute frame layout from ABI, regalloc results, and LPIR slots.
    /// 
    /// # Arguments
    /// * `abi` - Function ABI constraints
    /// * `spill_count` - Number of spill slots from regalloc
    /// * `used_callee_saved` - Which callee-saved registers were allocated
    /// * `lpir_slot_sizes` - LPIR slots as (slot_id, size_in_bytes)
    /// * `is_leaf` - Whether function calls other functions (affects RA save)
    pub fn compute(
        abi: &FuncAbi,
        spill_count: u32,
        used_callee_saved: PregSet,
        lpir_slot_sizes: &[(u32, u32)],
        is_leaf: bool,
    ) -> Self {
        // Start layout from the top (growing downward)
        let mut offset = 0i32;
        
        // 1. Saved registers area
        let mut callee_saves = Vec::new();
        let mut ra_offset = None;
        let mut fp_offset = None;
        
        // Save RA if non-leaf
        if !is_leaf {
            offset -= 4;
            ra_offset = Some(offset);
        }
        
        // Always save S0 (frame pointer) - simplifies addressing
        // In optimized leaf functions, could skip this
        offset -= 4;
        fp_offset = Some(offset);
        
        // Save used callee-saved registers
        for preg in used_callee_saved.iter() {
            offset -= 4;
            callee_saves.push((preg, offset));
        }
        
        // Record base of saved register area
        let saved_area_size = -offset;
        
        // 2. Spill area
        // Align to 4 bytes (spill slots are 4-byte aligned)
        let spill_base = offset;
        let spill_area_size = spill_count * 4;
        offset -= spill_area_size as i32;
        
        // 3. LPIR slot area
        // LPIR slots may have different sizes
        let lpir_base = offset;
        let mut lpir_slots = Vec::new();
        for (slot_id, size) in lpir_slot_sizes.iter().copied() {
            offset -= size as i32;
            lpir_slots.push((slot_id, offset));
        }
        let lpir_area_size = lpir_base - offset;
        
        // 4. Total size - 16-byte align
        let total_size = ((-offset) as u32 + 15) & !15;
        
        // Adjust offsets: they were relative to entry SP (0)
        // After prologue, SP = entry SP - total_size
        // So saved register offsets are at (total_size - saved_area_size)
        // Let's keep offsets relative to SP after prologue
        
        Self {
            total_size,
            ra_offset: ra_offset.map(|o| o + total_size as i32),
            fp_offset: fp_offset.map(|o| o + total_size as i32),
            callee_saves: callee_saves.into_iter()
                .map(|(r, o)| (r, o + total_size as i32))
                .collect(),
            spill_base: spill_base + total_size as i32,
            lpir_base: lpir_base + total_size as i32,
            spill_count,
            lpir_slots: lpir_slots.into_iter()
                .map(|(id, o)| (id, o + total_size as i32))
                .collect(),
        }
    }
    
    /// Get offset from SP for a spill slot.
    /// Slot 0 is at spill_base, slot 1 at spill_base - 4, etc.
    pub fn spill_offset(&self, index: u32) -> Option<i32> {
        if index < self.spill_count {
            Some(self.spill_base - (index * 4) as i32)
        } else {
            None
        }
    }
    
    /// Get offset from SP for an LPIR slot by ID.
    pub fn lpir_offset(&self, slot_id: u32) -> Option<i32> {
        self.lpir_slots.iter()
            .find(|(id, _)| *id == slot_id)
            .map(|(_, offset)| *offset)
    }
    
    /// Get offset from frame pointer (S0) for a spill slot.
    /// FP points to entry SP, so offset is positive going up.
    pub fn spill_offset_from_fp(&self, index: u32) -> Option<i32> {
        self.spill_offset(index)
            .map(|sp_offset| sp_offset + self.total_size as i32)
    }
    
    /// Get offset from frame pointer for an LPIR slot.
    pub fn lpir_offset_from_fp(&self, slot_id: u32) -> Option<i32> {
        self.lpir_offset(slot_id)
            .map(|sp_offset| sp_offset + self.total_size as i32)
    }
}
```

## Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi2::{FuncAbi, PregSet};
    use crate::abi2::rv32;

    fn simple_sig() -> LpsFnSig {
        LpsFnSig {
            name: "test".into(),
            return_type: LpsType::Scalar(ScalarType::Float),
            parameters: vec![],
        }
    }

    #[test]
    fn leaf_frame_saves_fp_only() {
        let sig = simple_sig();
        let abi = FuncAbi::new(&sig, 0);
        let frame = FrameLayout::compute(&abi, 0, PregSet::EMPTY, &[], true);
        
        assert!(frame.ra_offset.is_none());  // Leaf: no RA save
        assert!(frame.fp_offset.is_some());    // But FP always saved
        assert!(frame.callee_saves.is_empty());
    }

    #[test]
    fn non_leaf_frame_saves_ra_and_fp() {
        let sig = simple_sig();
        let abi = FuncAbi::new(&sig, 0);
        let frame = FrameLayout::compute(&abi, 0, PregSet::EMPTY, &[], false);
        
        assert!(frame.ra_offset.is_some());
        assert!(frame.fp_offset.is_some());
        // RA saved at higher address than FP
        assert!(frame.ra_offset.unwrap() > frame.fp_offset.unwrap());
    }

    #[test]
    fn used_callee_saved_are_saved() {
        let sig = simple_sig();
        let abi = FuncAbi::new(&sig, 0);
        let used = PregSet::singleton(rv32::S2).union(PregSet::singleton(rv32::S3));
        let frame = FrameLayout::compute(&abi, 0, used, &[], true);
        
        assert_eq!(frame.callee_saves.len(), 2);
        // S2 and S3 should have offsets
        let regs: Vec<_> = frame.callee_saves.iter().map(|(r, _)| *r).collect();
        assert!(regs.contains(&rv32::S2));
        assert!(regs.contains(&rv32::S3));
    }

    #[test]
    fn spill_slots_have_correct_offsets() {
        let sig = simple_sig();
        let abi = FuncAbi::new(&sig, 0);
        let frame = FrameLayout::compute(&abi, 3, PregSet::EMPTY, &[], true);
        
        // 3 spill slots
        assert_eq!(frame.spill_count, 3);
        
        // Slot 0 at spill_base
        let off0 = frame.spill_offset(0).unwrap();
        assert_eq!(off0, frame.spill_base);
        
        // Slot 1 at spill_base - 4
        let off1 = frame.spill_offset(1).unwrap();
        assert_eq!(off1, off0 - 4);
        
        // Slot 2 at spill_base - 8
        let off2 = frame.spill_offset(2).unwrap();
        assert_eq!(off2, off1 - 4);
        
        // Slot 3 doesn't exist
        assert!(frame.spill_offset(3).is_none());
    }

    #[test]
    fn lpir_slots_have_correct_offsets() {
        let sig = simple_sig();
        let abi = FuncAbi::new(&sig, 0);
        // 2 LPIR slots: slot 0 is 16 bytes, slot 1 is 8 bytes
        let lpir = vec![(0, 16), (1, 8)];
        let frame = FrameLayout::compute(&abi, 0, PregSet::EMPTY, &lpir, true);
        
        assert_eq!(frame.lpir_slots.len(), 2);
        
        // Both slots should be findable
        let off0 = frame.lpir_offset(0).unwrap();
        let off1 = frame.lpir_offset(1).unwrap();
        
        // Offsets are negative from SP
        assert!(off0 < 0);
        assert!(off1 < 0);
        
        // Slot 0 is larger, so likely at higher offset (closer to 0)
        // Actually depends on layout order - in compute() we go:
        // saved -> spills -> lpir, so lpir slots are lowest
    }

    #[test]
    fn frame_size_is_16_byte_aligned() {
        let sig = simple_sig();
        let abi = FuncAbi::new(&sig, 0);
        
        // Various configurations
        let frame1 = FrameLayout::compute(&abi, 0, PregSet::EMPTY, &[], true);
        let frame2 = FrameLayout::compute(&abi, 3, PregSet::EMPTY, &[], true);
        let frame3 = FrameLayout::compute(&abi, 5, PregSet::singleton(rv32::S2), &[], false);
        
        assert_eq!(frame1.total_size % 16, 0);
        assert_eq!(frame2.total_size % 16, 0);
        assert_eq!(frame3.total_size % 16, 0);
    }

    #[test]
    fn fp_offsets_are_positive() {
        // After prologue, SP is lower than entry
        // Offsets from SP are negative (or small positive near top)
        // But our compute() makes them relative to new SP, so should be positive
        let sig = simple_sig();
        let abi = FuncAbi::new(&sig, 0);
        let frame = FrameLayout::compute(&abi, 0, PregSet::EMPTY, &[], false);
        
        // RA and FP should be at positive offsets from new SP
        // (toward the top of the frame, toward entry SP)
        let ra_off = frame.ra_offset.unwrap();
        let fp_off = frame.fp_offset.unwrap();
        
        assert!(ra_off > 0);
        assert!(fp_off > 0);
        assert!(ra_off > fp_off);  // RA is higher (saved first)
    }

    #[test]
    fn fp_relative_offsets_work() {
        let sig = simple_sig();
        let abi = FuncAbi::new(&sig, 0);
        let frame = FrameLayout::compute(&abi, 2, PregSet::EMPTY, &[], true);
        
        // SP-relative
        let sp_off = frame.spill_offset(0).unwrap();
        // FP-relative should be more positive
        let fp_off = frame.spill_offset_from_fp(0).unwrap();
        
        assert!(fp_off > sp_off);
        assert_eq!(fp_off - sp_off, frame.total_size as i32);
    }
}
```

## Validate

```bash
cargo test -p lpvm-native abi::frame
```

All tests should pass. Verify:
- Leaf vs non-leaf RA handling
- Callee-saved register preservation
- Spill slot offset calculation
- LPIR slot placement
- 16-byte alignment
- Both SP and FP-relative offsets work
