## Phase 2: Frame Layout - Outgoing Stack Area

## Scope

Add `caller_arg_stack_size` to `FrameLayout` to reserve space for outgoing calls with >8 arguments.

## Code Organization Reminders

- `FrameLayout::compute` is a static method - place it near the top
- Add the new field to the struct definition
- Keep related fields together (frame sizing fields)

## Implementation Details

### 1. Add field to FrameLayout

**File**: `lp-shader/lpvm-native/src/abi/frame.rs`

Add to the struct:

```rust
pub struct FrameLayout {
    pub total_size: u32,
    pub save_ra: bool,
    pub save_fp: bool,
    pub ra_offset_from_sp: Option<i32>,
    pub fp_offset_from_sp: Option<i32>,
    pub callee_save_offsets: Vec<(PReg, i32)>,
    pub spill_count: u32,
    pub spill_base_from_sp: i32,
    pub lpir_slot_offsets: Vec<(u32, i32)>,
    pub sret_slot_size: u32,
    pub sret_slot_base_from_sp: i32,
    // NEW: Space for outgoing stack arguments
    pub caller_arg_stack_size: u32,
    /// SP-relative offset to caller arg area (positive = into caller's frame)
    pub caller_arg_base_from_sp: i32,
}
```

### 2. Compute max outgoing stack args

In `FrameLayout::compute`, add parameter and compute:

```rust
pub fn compute(
    _abi: &FuncAbi,
    spill_count: u32,
    used_callee_saved: PregSet,
    lpir_slot_sizes: &[(u32, u32)],
    is_leaf: bool,
    caller_sret_bytes: u32,
    max_outgoing_stack_args: u32, // NEW parameter
) -> Self {
    // ... existing computations ...
    
    // NEW: Caller arg stack area (for outgoing calls with >8 args)
    let caller_arg_stack_size = if max_outgoing_stack_args == 0 {
        0u32
    } else {
        (max_outgoing_stack_args.saturating_mul(4) + 15) & !15u32
    };
    
    // Position after spills + LPIR slots
    let body_bytes = spill_bytes
        .saturating_add(lpir_bytes)
        .saturating_add(sret_slot_size)
        .saturating_add(caller_arg_stack_size); // NEW
    
    // ... rest of computation ...
    
    let caller_arg_base_from_sp = (spill_bytes + lpir_bytes + sret_slot_size) as i32;
    
    Self {
        // ... existing fields ...
        caller_arg_stack_size,
        caller_arg_base_from_sp,
    }
}
```

### 3. Helper to compute max outgoing stack args

**File**: `lp-shader/lpvm-native/src/isa/rv32/emit.rs` (or new utility module)

Add function to scan VInsts and find max stack args needed:

```rust
fn max_outgoing_stack_args(vinsts: &[VInst], abi: &FuncAbi) -> u32 {
    let mut max = 0u32;
    for inst in vinsts {
        if let VInst::Call { args, callee_uses_sret, .. } = inst {
            let reg_count = if *callee_uses_sret { 7 } else { 8 };
            let stack_needed = args.len().saturating_sub(reg_count) as u32;
            max = max.max(stack_needed);
        }
    }
    max
}
```

### 4. Update emit_function_bytes

**File**: `lp-shader/lpvm-native/src/isa/rv32/emit.rs`

In `emit_function_bytes()`, compute and pass the value:

```rust
let max_outgoing = max_outgoing_stack_args(&vinsts, &func_abi);

let frame = FrameLayout::compute(
    &func_abi,
    alloc.spill_count().saturating_add(extra_spill),
    used_callee_saved,
    &lpir_slot_sizes,
    is_leaf,
    caller_sret_bytes,
    max_outgoing, // NEW
);
```

## Tests

Add test in `abi/frame.rs` `mod tests`:

```rust
#[test]
fn frame_with_outgoing_stack_args() {
    let sig = test_sig();
    let func_abi = func_abi_rv32(&sig, 1);
    
    let frame = FrameLayout::compute(
        &func_abi,
        0, // no spills
        PregSet::EMPTY,
        &[], // no LPIR slots
        false, // not leaf
        0, // no sret
        4, // 4 outgoing stack words (16 bytes aligned)
    );
    
    assert_eq!(frame.caller_arg_stack_size, 16); // 4 * 4 = 16, already aligned
    assert!(frame.total_size >= 16);
}
```

## Validate

```bash
cd /Users/yona/dev/photomancer/feature/lightplayer-native/lp-shader
cargo test -p lpvm-native --lib
```

Expected: All tests pass, frame layout tests verify new fields.
