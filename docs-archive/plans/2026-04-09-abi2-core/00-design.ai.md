# ABI2 Core Design

## Scope of Work

Create a new ABI system (abi2) for lpvm-native providing clean, testable, and correct abstractions for:

1. Register set management (`PReg`, `PRegSet` bitmasks with `RegClass`)
2. ISA-level ABI constants (RV32 ILP32 calling convention)
3. Per-function ABI classification (params, returns, sret detection)
4. Frame layout computation (spill slots + LPIR semantic slots)

The goal is textbook correctness: pure functions, testable in isolation, composes cleanly. Performance optimization is explicitly out of scope.

## Design Principles

1. **Two-layer architecture**: ISA-level rules (static) + per-function instances (computed)
2. **Pure functions**: Classification takes signature, returns locations - no mutation
3. **Immutable data**: FuncAbi is constructed once from signature, then queried
4. **Testability**: Each layer testable without full compiler context
5. **Type safety**: Newtype wrappers and enums instead of raw integers

## File Structure

```
lp-shader/lpvm-native/src/
├── isa/
│   └── rv32/
│       ├── mod.rs              # (existing, unchanged)
│       ├── inst.rs             # (existing, unchanged)
│       ├── emit.rs             # UPDATE: eventually use abi2
│       ├── abi.rs              # (existing, to be replaced)
│       └── abi2.rs             # NEW: ISA-specific register constants
├── abi2/
│   ├── mod.rs                  # NEW: Public API, re-exports
│   ├── regset.rs               # NEW: PReg, RegClass, PRegSet
│   ├── classify.rs             # NEW: classify_params, classify_return
│   ├── func_abi.rs             # NEW: FuncAbi struct
│   └── frame.rs                # NEW: FrameLayout, SlotKind
└── lib.rs                      # UPDATE: add abi2 module
```

## Conceptual Architecture

### Data Flow

```
LpsFnSig ──► classify_params() ──┐
                                   ├─► FuncAbi ──► regalloc uses ─┐
LpsFnSig ──► classify_return() ──┘        allocatable()         │
        │                              precolors()              │
        │                              param_loc()              │
        │                              is_sret()                  │
        │                                                         │
        └───────────────────────────────────────────────────────┴─► FrameLayout::compute()
                                                                    │
                                                                    ▼
                                                              (emission)
```

### Core Types

#### 1. Register Abstraction (`regset.rs`)

```rust
/// Physical register: encoding + class
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PReg {
    pub hw: u8,           // Hardware encoding (x0-x31 or f0-f31)
    pub class: RegClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegClass {
    Int,
    Float,  // For RV32F extension (unimplemented!() for now)
}

/// Compact register set (bitmask)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PRegSet(u32);  // 32 bits for RV32 (enough for int and float)

impl PRegSet {
    pub fn empty() -> Self;
    pub fn singleton(r: PReg) -> Self;
    pub fn contains(&self, r: PReg) -> bool;
    pub fn insert(&mut self, r: PReg);
    pub fn remove(&mut self, r: PReg);
    pub fn union(self, other: Self) -> Self;
    pub fn intersection(self, other: Self) -> Self;
    pub fn difference(self, other: Self) -> Self;
    pub fn count(&self) -> u32;
    pub fn iter(&self) -> impl Iterator<Item = PReg>;
}
```

#### 2. Classification Results (`classify.rs`)

```rust
/// Where a scalar parameter/return value lives
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgLoc {
    /// In a specific register
    Reg(PReg),
    /// On stack at offset from SP at call time
    Stack { offset: i32, size: u32 },
}

/// Return method classification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnMethod {
    Void,
    /// Values returned directly in registers
    Direct { locs: Vec<ArgLoc> },
    /// Values returned via caller-allocated buffer (sret)
    Sret {
        /// Register holding buffer pointer at entry (a0)
        ptr_reg: PReg,
        /// Callee-saved register for preservation (s1)
        preserved_reg: PReg,
        /// Number of scalar words
        word_count: u32,
    },
}

/// Pure classification functions
pub fn classify_params(sig: &LpsFnSig) -> Vec<ArgLoc>;
pub fn classify_return(sig: &LpsFnSig) -> ReturnMethod;
```

#### 3. Per-Function ABI (`func_abi.rs`)

```rust
/// Complete ABI description for one function.
/// Constructed from signature, then queried by regalloc and emission.
pub struct FuncAbi {
    params: Vec<ArgLoc>,
    return_method: ReturnMethod,
    // Computed sets for regalloc
    allocatable: PRegSet,
    precolors: Vec<(u32, PReg)>,  // (vreg_index, preg)
    caller_saved: PRegSet,
    callee_saved_source: PRegSet,  // Candidates for callee-save
}

impl FuncAbi {
    /// Construct from signature and param count
    pub fn new(sig: &LpsFnSig, total_param_slots: usize) -> Self;

    // -- Regalloc interface --
    pub fn allocatable(&self) -> PRegSet;
    pub fn precolors(&self) -> &[(u32, PReg)];
    pub fn call_clobbers(&self) -> PRegSet;

    // -- Emission interface --
    pub fn is_sret(&self) -> bool;
    pub fn sret_preservation_reg(&self) -> Option<PReg>;
    pub fn param_loc(&self, idx: usize) -> ArgLoc;
    pub fn return_locs(&self) -> &[ArgLoc];  // Empty if sret
    pub fn return_method(&self) -> &ReturnMethod;
}
```

#### 4. Frame Layout (`frame.rs`)

```rust
/// Kind of stack slot
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    /// Spill slot assigned by regalloc
    Spill { index: u32 },
    /// LPIR semantic slot (arrays, etc.)
    Lpir { index: u32, size: u32 },
}

/// Physical stack frame layout
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameLayout {
    pub total_size: u32,  // 16-byte aligned
    pub ra_offset: Option<i32>,  // Offset from SP, if saved
    pub fp_offset: Option<i32>,  // S0 save offset from SP, if saved
    pub callee_saves: Vec<(PReg, i32)>,  // (reg, offset from SP)
    pub spill_base: i32,  // Offset to spill slot 0
    pub lpir_base: i32,   // Offset to LPIR slot 0
    pub spill_count: u32,
    pub lpir_slots: Vec<(u32, i32)>,  // (slot_id, offset)
}

impl FrameLayout {
    /// Compute from ABI + regalloc results
    pub fn compute(
        abi: &FuncAbi,
        spill_count: u32,
        used_callee_saved: PRegSet,
        lpir_slots: &[(u32, u32)],  // (slot_id, size in bytes)
    ) -> Self;

    pub fn spill_offset(&self, index: u32) -> i32;
    pub fn lpir_offset(&self, slot_id: u32) -> Option<i32>;
}
```

### RV32 ISA Constants (`isa/rv32/abi2.rs`)

```rust
//! RV32 ILP32 ABI constants - ISA-level, no per-function state

use crate::abi2::{PReg, RegClass, PRegSet};

// Individual registers
pub const X0: PReg = PReg { hw: 0, class: RegClass::Int };   // zero
pub const RA: PReg = PReg { hw: 1, class: RegClass::Int };   // x1
pub const SP: PReg = PReg { hw: 2, class: RegClass::Int };   // x2
pub const S0: PReg = PReg { hw: 8, class: RegClass::Int };   // x8, frame pointer
pub const S1: PReg = PReg { hw: 9, class: RegClass::Int };   // x9, sret preservation
pub const A0: PReg = PReg { hw: 10, class: RegClass::Int };  // x10
pub const A1: PReg = PReg { hw: 11, class: RegClass::Int };  // x11
// ... etc

// Register sets (pre-computed PRegSets)
pub const ARG_REGS: [PReg; 8] = [A0, A1, A2, A3, A4, A5, A6, A7];
pub const RET_REGS: [PReg; 4] = [A0, A1, A2, A3];
pub const CALLER_SAVED: PRegSet = PRegSet::from_bits(0b...);  // a0-a7, t0-t6
pub const CALLEE_SAVED: PRegSet = PRegSet::from_bits(0b...);  // s0-s11
pub const ALL_ALLOCA: PRegSet = PRegSet::from_bits(0b...);    // t2, s1-s11, t3-t6
// Excludes: zero, ra, sp, a0-a7, t0-t1 (spill temps), s0 (frame pointer)
pub const RESERVED: PRegSet = PRegSet::from_bits(0b...);      // Not for allocation

// ABI parameters
pub const STACK_ALIGNMENT: u32 = 16;
pub const SRET_THRESHOLD: usize = 2;  // RV32: >2 scalars uses sret
```

## Interaction Patterns

### Register Allocator Queries

```rust
// Build ABI from signature
let abi = FuncAbi::new(&sig, total_param_slots);

// Get constraints
let allocatable = abi.allocatable();  // PregSet to assign from
let precolors = abi.precolors();      // (vreg, preg) pairs that are fixed

// After allocation
let used_callee_saved = compute_used_callee_saved(&allocation, &abi);
let frame = FrameLayout::compute(&abi, spill_count, used_callee_saved, &lpir_slots);
```

### Emitter Queries

```rust
// Prologue
if abi.is_sret() {
    // Emit: mv s1, a0  (preserve sret pointer)
}
// Save callee-saved regs from frame.callee_saves

// Parameter access
let loc = abi.param_loc(0);  // Where does param 0 arrive?

// Return
match abi.return_method() {
    ReturnMethod::Direct { locs } => {
        // Move values to locs[0], locs[1], ...
    }
    ReturnMethod::Sret { preserved_reg, word_count } => {
        // Store to buffer at preserved_reg
    }
    _ => {}
}
```

## Testing Strategy

Each layer testable in isolation:

```rust
// regset.rs tests
#[test]
preg_set_operations() { ... }

// classify.rs tests
#[test]
classify_scalar_returns_a0() {
    let sig = LpsFnSig { return_type: float, ... };
    let locs = classify_params(&sig);
    assert_eq!(locs[0], ArgLoc::Reg(rv32::A0));
}

#[test]
classify_mat4_is_sret() {
    let sig = LpsFnSig { return_type: mat4, ... };
    let ret = classify_return(&sig);
    assert!(matches!(ret, ReturnMethod::Sret { word_count: 16, .. }));
}

// func_abi.rs tests
#[test]
sret_excludes_s1_from_allocatable() {
    let abi = FuncAbi::new(&mat4_sig, 1);
    assert!(abi.is_sret());
    assert!(!abi.allocatable().contains(rv32::S1));
    // vmctx (vreg 0) should be pinned to A1, not A0
    assert_eq!(abi.precolors()[0], (0, rv32::A1));
}

#[test]
direct_includes_s1_in_allocatable() {
    let abi = FuncAbi::new(&float_sig, 1);
    assert!(!abi.is_sret());
    assert!(abi.allocatable().contains(rv32::S1));
    // vmctx pinned to A0
    assert_eq!(abi.precolors()[0], (0, rv32::A0));
}

// frame.rs tests
#[test]
frame_layout_computed_correctly() {
    let abi = FuncAbi::new(&sig, 1);
    let frame = FrameLayout::compute(&abi, 3, PRegSet::singleton(rv32::S2), &[]);
    assert_eq!(frame.spill_offset(0), frame.spill_base);
    assert_eq!(frame.spill_offset(1), frame.spill_base - 4);
}
```

## Prior Art Comparison

| Feature              | QBE                      | Cranelift                     | ABI2 (this design)         |
| -------------------- | ------------------------ | ----------------------------- | -------------------------- |
| Register abstraction | `RClass` bitmask         | `Reg` enum                    | `PReg` struct + `PregSet`  |
| Classification       | `classpar()`, `selret()` | `ABICaller::compute_arg_locs` | `classify_params/return()` |
| Per-function ABI     | `AClass[]` + struct info | `Callee` struct               | `FuncAbi` struct           |
| Frame layout         | `riscv64/stack.c`        | Integrated in `Callee`        | `FrameLayout::compute()`   |
| Stack slots          | Unified in frame         | Spill + stack slots           | `SlotKind` enum            |
| Testability          | Yes (unit tests)         | Hard (requires full ctx)      | Designed for testing       |

## Key Differences from Current System

1. **Register abstraction**: `u8` indices → `PReg` with class
2. **Sets as bitmasks**: `[PhysReg]` slices → `PregSet` for efficient operations
3. **Pure classification**: Mixed in `AbiInfo::from_lps_sig()` → separate `classify_*` functions
4. **Dynamic allocatable set**: Constant `ALLOCA_REGS` → `abi.allocatable()` that excludes s1 for sret
5. **Explicit precolors**: Implicit param→arg mapping → explicit `Vec<(vreg, preg)>`
6. **Unified frame**: Separate spill + slot handling → unified `FrameLayout` with `SlotKind`

## Migration Path

1. Build `abi2/` module with full test suite (this plan)
2. Create parallel emission path using abi2 (wire in, keep old as fallback)
3. Update regalloc to accept `PregSet` instead of `&[PhysReg]`
4. Switch filetests to abi2 path, verify all pass
5. Delete `abi.rs`, rename `abi2/` → `abi/`
