# Phase 4: Update FrameLayout for Caller-Side Sret Slot

## Scope of Phase

Update `FrameLayout` to include a pre-allocated slot for caller-side sret buffers. This slot is used when calling functions that return more than 2 scalars.

## Code Organization Reminders

- Keep `FrameLayout` struct definition at the top of the file
- Update `compute()` method to accept `caller_sret_bytes` parameter
- Update `compute_layout()` helper to include sret slot
- Add tests at the bottom

## Implementation Details

### File: `lp-shader/lpvm-native/src/abi/frame.rs`

**Update `FrameLayout` struct:**

```rust
/// Layout of the stack frame for a single function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameLayout {
    pub total_size: u32,
    pub spill_start_offset: i32,
    pub spill_count: u32,
    pub ra_offset_from_sp: Option<i32>,
    pub fp_offset_from_sp: Option<i32>,
    pub save_fp: bool,
    /// Offset from frame pointer for pre-allocated caller-side sret buffer.
    /// `None` if no callees use sret (sret_slot_size == 0).
    pub sret_slot_offset: Option<i32>,  // NEW
    /// Size in bytes of the pre-allocated sret slot (0 if not needed).
    pub sret_slot_size: u32,            // NEW
}
```

**Update `FrameLayout::compute` signature:**

```rust
impl FrameLayout {
    /// Compute frame layout for a function.
    ///
    /// # Arguments
    /// * `func_abi` - ABI info for this function
    /// * `spill_count` - Number of spill slots needed
    /// * `used_callee_saved` - Set of callee-saved registers this function uses
    /// * `clobber_callee_saved` - Additional callee-saved registers clobbered by calls
    /// * `is_leaf` - Whether this function makes any calls
    /// * `caller_sret_bytes` - NEW: max sret buffer size needed for any callee (0 if none)
    pub fn compute(
        func_abi: &FuncAbi,
        spill_count: u32,
        used_callee_saved: PregSet,
        clobber_callee_saved: &[PReg],
        is_leaf: bool,
        caller_sret_bytes: u32,  // NEW
    ) -> Self {
        let layout = compute_layout(
            func_abi,
            spill_count,
            used_callee_saved,
            clobber_callee_saved,
            is_leaf,
            caller_sret_bytes,  // NEW
        );
        Self::from_parts(layout)
    }
}
```

**Update `compute_layout` to include sret slot:**

```rust
fn compute_layout(
    func_abi: &FuncAbi,
    spill_count: u32,
    used_callee_saved: PregSet,
    clobber_callee_saved: &[PReg],
    is_leaf: bool,
    caller_sret_bytes: u32,  // NEW
) -> LayoutParts {
    let alignment = func_abi.stack_alignment();
    let word_size = 4u32;
    
    // --- Determine what needs to be saved ---
    let needs_ra = !is_leaf;
    let needs_fp = !is_leaf || used_callee_saved.contains(crate::isa::rv32::abi::S0);
    
    // --- Calculate sizes ---
    let ra_size = if needs_ra { word_size } else { 0 };
    let fp_size = if needs_fp { word_size } else { 0 };
    let callee_saved_size = word_size * clobber_callee_saved.len() as u32;
    let spill_area_size = word_size * spill_count;
    let sret_slot_size = caller_sret_bytes;  // NEW
    
    // --- Layout from high to low addresses (growing downward) ---
    // [higher addresses]
    //   saved ra (if non-leaf)
    //   saved fp/s0 (if needed)
    //   callee-saved registers
    //   sret slot (if needed for callees)  <-- NEW
    //   spill slots
    // [lower addresses = sp after prologue]
    
    let mut offset = 0i32;
    
    // ra is saved first (at lowest offset within saved area)
    let ra_offset = if needs_ra {
        offset -= word_size as i32;
        Some(offset)
    } else {
        None
    };
    
    // fp/s0 saved next
    let fp_offset = if needs_fp {
        offset -= word_size as i32;
        Some(offset)
    } else {
        None
    };
    
    // callee-saved registers
    for _ in clobber_callee_saved {
        offset -= word_size as i32;
    }
    let callee_saved_end = offset;
    
    // sret slot (aligned to word boundary)  <-- NEW
    let sret_offset = if sret_slot_size > 0 {
        // Align sret slot size
        let aligned_sret_size = (sret_slot_size + word_size - 1) & !(word_size - 1);
        offset -= aligned_sret_size as i32;
        Some(offset)
    } else {
        None
    };
    
    // spill area
    let spill_start = offset;
    offset -= spill_area_size as i32;
    
    // Align total frame size
    let total_size = ((-offset) as u32 + alignment - 1) & !(alignment - 1);
    
    LayoutParts {
        total_size,
        spill_start_offset: spill_start,
        spill_count,
        ra_offset: ra_offset,
        fp_offset: fp_offset,
        save_fp: needs_fp,
        sret_slot_offset: sret_offset,  // NEW
        sret_slot_size,                 // NEW
    }
}
```

**Update `LayoutParts` struct:**

```rust
struct LayoutParts {
    total_size: u32,
    spill_start_offset: i32,
    spill_count: u32,
    ra_offset: Option<i32>,
    fp_offset: Option<i32>,
    save_fp: bool,
    sret_slot_offset: Option<i32>,  // NEW
    sret_slot_size: u32,             // NEW
}
```

**Update `from_parts`:**

```rust
fn from_parts(p: LayoutParts) -> FrameLayout {
    Self {
        total_size: p.total_size,
        spill_start_offset: p.spill_start_offset,
        spill_count: p.spill_count,
        ra_offset_from_sp: p.ra_offset,
        fp_offset_from_sp: p.fp_offset,
        save_fp: p.save_fp,
        sret_slot_offset: p.sret_slot_offset,  // NEW
        sret_slot_size: p.sret_slot_size,       // NEW
    }
}
```

**Add helper method:**

```rust
impl FrameLayout {
    // ... existing methods ...
    
    /// Offset from frame pointer to the pre-allocated caller-side sret buffer.
    /// Returns `None` if no sret slot was allocated.
    pub fn sret_slot_offset_from_fp(&self) -> Option<i32> {
        self.sret_slot_offset
    }
    
    /// Size in bytes of the pre-allocated sret slot.
    pub fn sret_slot_size(&self) -> u32 {
        self.sret_slot_size
    }
}
```

### Update Call Sites

Find all places that call `FrameLayout::compute`:

1. **In `emit.rs` `EmitContext::new` (line 107):**

```rust
// Currently:
let frame = FrameLayout::compute(&func_abi, 0, PregSet::EMPTY, &[], is_leaf);

// Update to:
let frame = FrameLayout::compute(&func_abi, 0, PregSet::EMPTY, &[], is_leaf, 0);
```

2. **In `emit.rs` `emit_function_bytes` (line 751-757):**

```rust
// Currently:
let frame = FrameLayout::compute(
    &func_abi,
    alloc.spill_count(),
    used_callee_saved,
    &[],
    is_leaf,
);

// Update to (will use ModuleAbi in next phase):
let caller_sret_bytes = 0u32;  // TODO: get from ModuleAbi
let frame = FrameLayout::compute(
    &func_abi,
    alloc.spill_count(),
    used_callee_saved,
    &[],
    is_leaf,
    caller_sret_bytes,
);
```

3. **In `abi/frame.rs` tests:**

Update all `FrameLayout::compute` calls to include the new parameter (use `0` for tests that don't care about sret).

### Tests to Add

```rust
#[test]
fn frame_with_caller_sret_slot() {
    use crate::abi::classify::{ArgLoc, ReturnMethod};
    
    let func_abi = FuncAbi::new_raw(
        vec![ArgLoc::Reg(PReg { hw: 10, class: RegClass::Int })],  // vmctx in a0
        ReturnMethod::Void,
        PregSet::EMPTY,
        vec![],
        PregSet::EMPTY,
        PregSet::EMPTY,
    );
    
    let frame = FrameLayout::compute(&func_abi, 0, PregSet::EMPTY, &[], false, 64);
    
    assert_eq!(frame.sret_slot_size(), 64);
    assert!(frame.sret_slot_offset_from_fp().is_some());
    assert!(frame.total_size >= 64 + 16);  // sret + alignment
}

#[test]
fn frame_without_caller_sret_slot() {
    let func_abi = FuncAbi::new_raw(
        vec![ArgLoc::Reg(PReg { hw: 10, class: RegClass::Int })],
        ReturnMethod::Void,
        PregSet::EMPTY,
        vec![],
        PregSet::EMPTY,
        PregSet::EMPTY,
    );
    
    let frame = FrameLayout::compute(&func_abi, 0, PregSet::EMPTY, &[], true, 0);
    
    assert_eq!(frame.sret_slot_size(), 0);
    assert!(frame.sret_slot_offset_from_fp().is_none());
}
```

## Validate

```bash
cargo test -p lpvm-native frame
# Note: emit.rs will have compile errors until Phase 5 completes
cargo check -p lpvm-native --lib  # Check lib only, skip emit.rs for now
```

Ensure:
- FrameLayout tests pass
- No compiler warnings in abi module
