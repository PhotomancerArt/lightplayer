# Phase 2: Compact VReg, VRegSlice, SymbolId Types

## Scope

Define the new compact types that replace the bloated existing ones:
- `VReg(pub u16)` — local to vinst.rs, separate from lpir::VReg
- `VRegSlice` — slice into vreg_pool for Call/Ret operands
- `SymbolId` — index into ModuleSymbols for callee names

## Implementation

### 1. Update `vinst.rs` - Type Definitions

Replace the existing type section:

```rust
//! Virtual instructions: post-lowering, pre-regalloc.
//!
//! Memory-optimized design:
//! - VReg is u16 (65,536 max vregs, enough for any embedded shader)
//! - VRegSlice for Call/Ret operands (no Vec heap allocation)
//! - SymbolId for callee names (module-level interning)
//! - src_op is u16 with 0xFFFF sentinel (not Option<u32>)

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Virtual register index - compact u16, local to native backend.
/// Note: This is separate from lpir::VReg (which is u32).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Ord, PartialOrd)]
pub struct VReg(pub u16);

/// Slice into the vreg_pool for Call/Ret operands.
/// Single allocation per function instead of per-Call Vec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VRegSlice {
    /// Index into LoweredFunction.vreg_pool.
    pub start: u16,
    /// Number of vregs in the slice (max 255, ABI limit is 8).
    pub count: u8,
}

impl VRegSlice {
    /// Empty slice (count = 0).
    pub const EMPTY: Self = Self { start: 0, count: 0 };

    /// Create new slice.
    pub fn new(start: u16, count: u8) -> Self {
        Self { start, count }
    }

    /// Check if slice is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Get slice length.
    pub fn len(&self) -> usize {
        self.count as usize
    }

    /// Iterate over vregs in this slice using the pool.
    pub fn iter<'a>(&self, pool: &'a [VReg]) -> impl Iterator<Item = VReg> + 'a {
        let start = self.start as usize;
        let end = start + self.count as usize;
        pool[start..end].iter().copied()
    }
}

/// Symbol identifier - index into ModuleSymbols.names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SymbolId(pub u16);

/// Sentinel value for "no symbol" (maps to empty string).
pub const SYMBOL_ID_NONE: u16 = u16::MAX;

/// Label id for branch targets.
pub type LabelId = u16;  // Changed from u32

/// src_op sentinel value meaning "no source op".
pub const SRC_OP_NONE: u16 = u16::MAX;

// ... rest of file (IcmpCond, etc.)
```

### 2. Add conversion helper

```rust
/// Convert lpir::VReg to native VReg.
/// Panics if the vreg index exceeds u16::MAX.
pub fn lower_vreg(v: lpir::VReg) -> VReg {
    VReg(u16::try_from(v.0).expect("vreg index exceeds u16::MAX"))
}
```

### 3. Update `lib.rs`

Export the new types:

```rust
pub mod vinst;
pub use vinst::{VReg, VRegSlice, SymbolId, LabelId, SRC_OP_NONE, SYMBOL_ID_NONE};
```

## Code Organization Reminders

- Place type definitions at the top of vinst.rs
- Place conversion helper after type definitions
- Add unit tests at the bottom in `mod tests`

## Validate

```bash
cargo check -p lpvm-native-fa --lib
```

Should compile. The VInst enum still uses old types — that comes next phase.
