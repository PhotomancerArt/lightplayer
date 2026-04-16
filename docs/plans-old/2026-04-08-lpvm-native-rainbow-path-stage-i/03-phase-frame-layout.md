## Scope of Phase

Finalize frame layout computation integrating spills and stack slots. Add helper methods for offset calculation.

## Code Organization Reminders

- Extend `FrameLayout` with complete computation
- Add `spill_to_offset()` method
- Keep frame size computation in one place

## Implementation Details

### Complete FrameLayout

```rust
#[derive(Debug, Clone)]
pub struct FrameLayout {
    /// Total frame size (16-byte aligned)
    pub total_size: u32,
    /// Whether to save ra
    pub saved_ra: bool,
    /// Whether to save s0 (always true with our design)
    pub saved_s0: bool,
    /// Number of spill slots assigned
    pub spill_count: u32,
    /// Stack slots for sret/out-params
    pub stack_slots: Vec<StackSlot>,
    /// Offset where s0 is saved (always 0 relative to sp after prologue)
    pub s0_save_offset: i32,
    /// Offset where ra is saved
    pub ra_save_offset: i32,
}

impl FrameLayout {
    pub fn compute(func: &IrFunction, spill_count: u32) -> Self {
        // Fixed header: saved s0 + saved ra = 8 bytes
        let header_size = 8u32;
        
        // Stack slots space
        let stack_space: u32 = func.slots.iter()
            .map(|s| (s.size + 3) & !3) // 4-byte align each
            .sum();
        
        // Spill space: 4 bytes per spill slot
        let spill_space = spill_count * 4;
        
        // Total, rounded to 16-byte alignment
        let total = (header_size + stack_space + spill_space + 15) & !15;
        
        Self {
            total_size: total,
            saved_ra: !func.is_leaf(),
            saved_s0: true,
            spill_count,
            stack_slots: Self::compute_stack_slots(&func.slots),
            s0_save_offset: 0,   // s0 saved at sp+0 after prologue
            ra_save_offset: 4,   // ra saved at sp+4
        }
    }
    
    /// Convert spill slot index to s0-relative offset
    /// Slot 0 = -8, Slot 1 = -12, Slot 2 = -16, ...
    pub fn spill_to_offset(&self, slot_index: u32) -> i32 {
        assert!(slot_index < self.spill_count);
        -((8 + slot_index * 4) as i32)
    }
    
    /// Stack slot offset from s0 (negative)
    pub fn stack_slot_offset(&self, slot_index: u32) -> i32 {
        assert!((slot_index as usize) < self.stack_slots.len());
        // Stack slots come after spill area
        let spill_space = self.spill_count * 4;
        -((8 + spill_space + slot_index * 4) as i32)
    }
}
```

## Tests to Write

```rust
#[test]
fn spill_offsets_are_negative() {
    let layout = FrameLayout::compute(&empty_func(), 3);
    assert_eq!(layout.spill_to_offset(0), -8);
    assert_eq!(layout.spill_to_offset(1), -12);
    assert_eq!(layout.spill_to_offset(2), -16);
}

#[test]
fn frame_size_is_16_aligned() {
    // Various spill counts should all produce 16-aligned sizes
    for spills in 0..10 {
        let layout = FrameLayout::compute(&empty_func(), spills);
        assert_eq!(layout.total_size % 16, 0);
    }
}

#[test]
fn leaf_function_still_has_frame() {
    let mut func = empty_func();
    func.body = vec![]; // no calls = leaf
    let layout = FrameLayout::compute(&func, 0);
    // Even leaf needs s0 saved for frame pointer
    assert!(layout.saved_s0);
    assert!(!layout.saved_ra); // ra not saved in leaf
}
```

## Validate

```bash
cargo test -p lpvm-native frame
cargo check -p lpvm-native
```
