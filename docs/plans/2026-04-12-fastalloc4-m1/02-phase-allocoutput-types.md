# Phase 2: Define AllocOutput Types

## Scope

Define the new allocator output types following regalloc2's structure:
- `Alloc` — where an operand lives
- `AllocOutput` — per-operand allocations + edit list
- `EditPoint` — position relative to a VInst
- `Edit` — move between allocations

## Code Organization

Add these types to `fa_alloc/mod.rs` near the top, replacing the current
`run_shell()` and broken `allocate()` stubs.

## Implementation

In `fa_alloc/mod.rs`, add:

```rust
/// Where an operand lives: physical register, spill slot, or unassigned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alloc {
    /// Assigned to this physical register.
    Reg(PReg),
    /// Spilled to this slot (0-based, FP-relative).
    Stack(u8),
    /// Unassigned (shouldn't happen after successful allocation).
    None,
}

impl Alloc {
    pub fn is_reg(self) -> bool {
        matches!(self, Alloc::Reg(_))
    }
    pub fn is_stack(self) -> bool {
        matches!(self, Alloc::Stack(_))
    }
    pub fn reg(self) -> Option<PReg> {
        match self {
            Alloc::Reg(r) => Some(r),
            _ => None,
        }
    }
    pub fn stack_slot(self) -> Option<u8> {
        match self {
            Alloc::Stack(s) => Some(s),
            _ => None,
        }
    }
}

/// Position relative to a VInst where an edit is inserted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EditPoint {
    Before(u16),  // VInst index
    After(u16),
}

impl EditPoint {
    pub fn inst(self) -> u16 {
        match self {
            EditPoint::Before(i) | EditPoint::After(i) => i,
        }
    }
}

/// An edit: move value from one allocation to another.
/// Covers spill (reg → stack), reload (stack → reg), and reg-reg moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    Move { from: Alloc, to: Alloc },
}

/// Allocator output: per-operand assignments and edits to insert.
/// Following regalloc2's `Output` structure.
pub struct AllocOutput {
    /// Flat array of allocations: allocs[(inst_idx, operand_idx)].
    /// Use `inst_alloc_offsets` to find the start for each instruction.
    pub allocs: Vec<Alloc>,

    /// Offset into `allocs` for each instruction's operands.
    /// `inst_alloc_offsets[i]` is the index where instruction i's allocations start.
    pub inst_alloc_offsets: Vec<u16>,

    /// Edits to insert between instructions, sorted by EditPoint.
    pub edits: Vec<(EditPoint, Edit)>,

    /// Total spill slots needed for this function.
    pub num_spill_slots: u32,

    /// Allocator trace for debugging.
    pub trace: AllocTrace,
}

impl AllocOutput {
    /// Get the allocation for a specific operand of an instruction.
    pub fn operand_alloc(&self, inst: u16, operand_idx: u16) -> Alloc {
        let offset = self.inst_alloc_offsets[inst as usize];
        self.allocs[offset as usize + operand_idx as usize]
    }

    /// Set the allocation for a specific operand.
    pub fn set_operand_alloc(&mut self, inst: u16, operand_idx: u16, alloc: Alloc) {
        let offset = self.inst_alloc_offsets[inst as usize];
        self.allocs[offset as usize + operand_idx as usize] = alloc;
    }
}

/// Allocation error types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllocError {
    NotImplemented,
    TooManyVRegs,
    UnsupportedControlFlow,
    OutOfRegisters,
}

impl fmt::Display for AllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AllocError::NotImplemented => write!(f, "allocator not yet implemented (M1)"),
            AllocError::TooManyVRegs => write!(f, "too many virtual registers"),
            AllocError::UnsupportedControlFlow => write!(f, "unsupported control flow"),
            AllocError::OutOfRegisters => write!(f, "out of registers"),
        }
    }
}

impl core::error::Error for AllocError {}
```

## Stub Allocator

Replace the current `run_shell()` and `allocate()` with:

```rust
/// Stub allocator: returns NotImplemented error.
/// M2 will implement the real backward walk.
pub fn allocate(
    _lowered: &LoweredFunction,
    _func_abi: &FuncAbi,
) -> Result<AllocOutput, AllocError> {
    Err(AllocError::NotImplemented)
}
```

Remove `run_shell()` entirely — it was a debugging helper for the old broken walk.

## Code Organization Reminders

- Place types at the top of the file, before any functions.
- Place `allocate()` stub near the bottom of the file, after the types.
- Keep imports clean — we'll fix broken ones in Phase 5.

## Implementation

1. Add `Alloc`, `EditPoint`, `Edit` enums to `fa_alloc/mod.rs`
2. Add `AllocOutput` struct with helper methods
3. Add `AllocError` enum with `NotImplemented` variant
4. Replace `run_shell()` and old `allocate()` with stub

## Validation

```bash
cargo check -p lpvm-native-fa 2>&1 | head -30
```

Expected: Fewer errors than Phase 1. The AllocOutput types should compile. Other
errors about missing modules remain (we'll fix in later phases).

## Temporary Code

The `NotImplemented` error is temporary — M2 will implement real allocation.
