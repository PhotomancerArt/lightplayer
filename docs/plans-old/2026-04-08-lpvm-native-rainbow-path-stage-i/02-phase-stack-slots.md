## Scope of Phase

Implement LPIR stack slot layout computation. Stack slots are used for:
- sret buffers (caller-allocated return areas)
- Local arrays/structs too large for registers
- Out-parameters for builtins

## Code Organization Reminders

- Add `StackSlot` tracking to `FrameLayout`
- Compute offsets from frame base (s0)
- Keep slot allocation simple: sequential assignment

## Implementation Details

### StackSlot struct

```rust
#[derive(Debug, Clone, Copy)]
pub struct StackSlot {
    pub index: u32,
    pub size: u32,
    pub align: u32,
}

impl StackSlot {
    /// Offset from s0 (negative, below frame pointer)
    pub fn offset_from_s0(&self) -> i32 {
        // Stack slots grow downward from s0
        // s0-8 = first slot, s0-12 = second, etc.
        -((8 + self.index * 4) as i32)
    }
}
```

### FrameLayout extension

```rust
pub struct FrameLayout {
    pub total_size: u32,
    pub saved_ra: bool,
    pub saved_s0: bool,
    pub spill_count: u32,
    pub stack_slots: Vec<StackSlot>,
}

impl FrameLayout {
    pub fn new(func: &IrFunction, spill_count: u32) -> Self {
        // Assign stack slots from LPIR
        let mut slots = Vec::new();
        for (i, slot) in func.slots.iter().enumerate() {
            slots.push(StackSlot {
                index: i as u32,
                size: slot.size,
                align: slot.align,
            });
        }
        
        // Compute total frame size
        // minimum 16 bytes, round up to 16-byte alignment
        let slot_space = slots.iter().map(|s| s.size).sum::<u32>();
        let spill_space = spill_count * 4;
        let total = (16 + slot_space + spill_space + 15) & !15;
        
        Self {
            total_size: total,
            saved_ra: func.calls_other_functions(),
            saved_s0: true, // always use frame pointer
            spill_count,
            stack_slots: slots,
        }
    }
    
    /// Get offset for a stack slot (negative from s0)
    pub fn stack_slot_offset(&self, slot_index: u32) -> i32 {
        -((8 + slot_index * 4) as i32)
    }
}
```

## Tests to Write

```rust
#[test]
fn frame_layout_with_stack_slots() {
    let func = IrFunction {
        slots: vec![
            Slot { size: 16, align: 4 }, // sret buffer for mat4
            Slot { size: 8, align: 4 },  // out-param buffer
        ],
        ..default()
    };
    let layout = FrameLayout::new(&func, 0);
    assert_eq!(layout.stack_slots.len(), 2);
    assert_eq!(layout.stack_slot_offset(0), -8);
    assert_eq!(layout.stack_slot_offset(1), -12);
}

#[test]
fn frame_layout_with_spills_and_slots() {
    let func = IrFunction {
        slots: vec![Slot { size: 16, align: 4 }],
        ..default()
    };
    let layout = FrameLayout::new(&func, 3); // 3 spill slots
    // Total: 16 (min) + 16 (slot) + 12 (spills) = 44 -> rounded to 48
    assert!(layout.total_size >= 48);
}
```

## Validate

```bash
cargo test -p lpvm-native frame_layout
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf
```
