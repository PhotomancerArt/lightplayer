# M3.1: Memory-Optimized Refactoring - Design

## Scope of Work

Shrink VInst from ~88 bytes to ~20 bytes per instruction, eliminate heap allocations in Call/Ret, define memory-efficient region tree and RegSet types. Work happens in `lpvm-native-fa` crate — clean fork with no legacy compatibility burden.

## File Structure

```
lp-shader/lpvm-native-fa/src/
├── config.rs                    # UPDATE: Add MAX_VREGS constant
├── types.rs                     # EXISTING
├── vinst.rs                     # MAJOR UPDATE: u16 VReg, VRegSlice, SymbolId
├── lower.rs                     # MAJOR UPDATE: ModuleSymbols, vreg_pool, new types
├── regset.rs                    # NEW: RegSet bitset type
├── region.rs                    # NEW: Region tree arena structure
├── lib.rs                       # UPDATE: Export new types
├── debug/
│   └── vinst.rs                 # UPDATE: Formatters for new types
├── isa/
│   └── rv32/
│       ├── alloc.rs             # UPDATE: Use new types (mechanical)
│       └── debug/
│           └── pinst.rs         # UPDATE: Formatters for new types
└── regalloc/
    ├── greedy.rs                # UPDATE: Use for_each_def/use (mechanical)
    └── linear_scan.rs           # UPDATE: Use for_each_def/use (mechanical)
```

## Conceptual Architecture

```
LPIR (lpir::VReg = u32)
    ↓ lower_ops()
┌─────────────────────────────────────────────────────────────┐
│ Lowering Context                                            │
│  - ModuleSymbols: interned callee names (SymbolId → String) │
│  - vreg_pool: Vec<VReg> for Call/Ret operands               │
│  - out: Vec<VInst> with u16 VReg, u16 src_op                │
└─────────────────────────────────────────────────────────────┘
    ↓
LoweredModule {
    functions: Vec<LoweredFunction>,
    symbols: ModuleSymbols,      // shared across functions
}

LoweredFunction {
    vinsts: Vec<VInst>,         // compact enum (~20 bytes each)
    vreg_pool: Vec<VReg>,       // single alloc for all Call/Ret operands
    regions: RegionTree,        // arena-based, built during lowering (M4)
    loop_regions: Vec<LoopRegion>,
}
```

### VInst Memory Layout

| Variant | Old Size | New Size | Change |
|---------|----------|----------|--------|
| Add32 | ~88 bytes | ~14 bytes | 3× u16 VReg + u16 src_op |
| Call | ~88 bytes | ~18 bytes | SymbolId + VRegSlice × 2 + u16 src_op |
| Ret | ~88 bytes | ~10 bytes | VRegSlice + u16 src_op |
| IConst32 | ~88 bytes | ~10 bytes | u16 VReg + i32 const + u16 src_op |

**Total savings:** ~68 bytes per instruction. For 100 instructions: 8.8 KB → 2.0 KB.

### Key Types

```rust
// vinst.rs - compact virtual register type
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Ord, PartialOrd)]
pub struct VReg(pub u16);

// Slice into vreg_pool for Call/Ret operands
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VRegSlice {
    pub start: u16,   // index into LoweredFunction.vreg_pool
    pub count: u8,    // max 255 args (ABI limit is 8)
}

// Symbol interning
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SymbolId(pub u16);

pub struct ModuleSymbols {
    pub names: Vec<String>,  // interned callee names
}

// regset.rs - fixed-size bitset for liveness
pub const MAX_VREGS: usize = 256;
pub const VREG_WORDS: usize = MAX_VREGS / 64;  // 4

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegSet(pub [u64; VREG_WORDS]);

// region.rs - arena-based region tree (for M4)
pub type RegionId = u16;

pub enum Region {
    Linear { start: u16, end: u16 },
    IfThenElse { head: RegionId, then_body: RegionId, else_body: RegionId },
    Loop { header: RegionId, body: RegionId },
    Seq { children_start: u16, child_count: u16 },
}

pub struct RegionTree {
    pub nodes: Vec<Region>,
    pub seq_children: Vec<RegionId>,
    pub root: RegionId,
}
```

### defs()/uses() API Change

Old (allocating):
```rust
pub fn defs(&self) -> impl Iterator<Item = VReg> + '_
pub fn uses(&self) -> impl Iterator<Item = VReg> + '_
```

New (zero allocation):
```rust
pub fn for_each_def<F: FnMut(VReg)>(&self, pool: &[VReg], f: F);
pub fn for_each_use<F: FnMut(VReg)>(&self, pool: &[VReg], f: F);
```

## Main Components

### 1. `config.rs` - MAX_VREGS constant

Add the compile-time constant that controls RegSet size:

```rust
/// Maximum number of virtual registers supported by the allocator.
/// Determines RegSet size (MAX_VREGS / 64 words).
pub const MAX_VREGS: usize = 256;
```

### 2. `vinst.rs` - Compact VInst enum

Replace the existing VInst enum with:
- `VReg(pub u16)` instead of re-exporting `lpir::VReg`
- `VRegSlice` for Call/Ret operand lists
- `SymbolId` instead of `SymbolRef { String }`
- `src_op: u16` with `0xFFFF` sentinel instead of `Option<u32>`

### 3. `regset.rs` - Bitset for liveness

Define the fixed-size bitset that replaces `BTreeSet<VReg>`:

```rust
impl RegSet {
    pub fn new() -> Self;
    pub fn insert(&mut self, vreg: VReg);
    pub fn remove(&mut self, vreg: VReg);
    pub fn contains(&self, vreg: VReg) -> bool;
    pub fn union(&self, other: &RegSet) -> RegSet;
    pub fn is_empty(&self) -> bool;
    pub fn iter(&self) -> impl Iterator<Item = VReg>;
}
```

### 4. `region.rs` - Arena-based region tree

Define the region tree structure that will be built during lowering in M4. This milestone just defines the types; actual building happens in M4.

### 5. `lower.rs` - Updated lowering with pools

Extend lowering to build:
- `ModuleSymbols` — intern callee names as they're encountered
- `vreg_pool` — append Call args and Ret vals as slices
- `VRegSlice` — point into pool instead of owning Vec

Add `LoweredModule` as the top-level result:

```rust
pub struct LoweredModule {
    pub functions: Vec<LoweredFunction>,
    pub symbols: ModuleSymbols,
}
```

### 6. `regalloc/` - Mechanical updates

Update `greedy.rs` and `linear_scan.rs` to use:
- New `VReg(u16)` type
- `for_each_def()` / `for_each_use()` instead of `defs()` / `uses()`

These allocators don't need optimization — just need to compile with new types.

### 7. Debug formatters

Update `debug/vinst.rs` and `isa/rv32/debug/pinst.rs` to:
- Format new VInst shapes
- Show VRegSlice contents by looking up in pool
- Show SymbolId by looking up in ModuleSymbols

## Success Criteria

1. `cargo check -p lpvm-native-fa` passes with no errors
2. `cargo test -p lpvm-native-fa` passes
3. `VInst` enum size ≤ 24 bytes (verified with `std::mem::size_of` test)
4. `RegSet` size = 32 bytes (4 × u64)
5. No heap allocations during `for_each_def()` / `for_each_use()` calls

## Phases

1. Add MAX_VREGS constant and RegSet type
2. Define compact VReg, VRegSlice, SymbolId types
3. Update VInst enum with new compact shape
4. Add ModuleSymbols and update lowering
5. Replace defs()/uses() with for_each_def()/for_each_use()
6. Update debug formatters
7. Update regalloc modules (greedy, linear_scan, rv32/alloc)
8. Define RegionTree structure
9. Cleanup and validation
