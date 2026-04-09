# Phase 6: abi2 Module Integration

## Scope

Create `abi2/mod.rs` that ties all the abi2 components together and re-exports the public API.

## Code Organization

- Module declarations
- Public re-exports
- Doc comment with overview

## Implementation

```rust
//! ABI2: Clean, testable, correct ABI abstraction for lpvm-native.
//!
//! Two-layer architecture:
//! 1. **ISA-level rules**: `rv32` module - static constants for RV32 ILP32 calling convention
//! 2. **Per-function ABI**: `FuncAbi` - constructed from signature, consumed by regalloc and emission
//!
//! Data flow:
//! ```ignore
//! LpsFnSig → classify_params/return → FuncAbi → regalloc → FrameLayout::compute → emission
//! ```

pub mod regset;
pub mod classify;
pub mod func_abi;
pub mod frame;

// Re-exports for convenient access
pub use regset::{PReg, RegClass, PregSet};
pub use classify::{ArgLoc, ReturnMethod, classify_params, classify_return};
pub use func_abi::FuncAbi;
pub use frame::{FrameLayout, SlotKind};

// ISA-specific constants are in isa::rv32::abi
// Re-exported there as pub use abi::* plus rv32-specific sets
```

## isa/rv32/abi2.rs Update

Update the file to re-export abi2 types and add RV32-specific constants:

```rust
//! RV32 ILP32 ABI constants for abi.
//!
//! This module provides RV32-specific register constants and pre-built
//! register sets for the abi system.

pub use crate::abi2::*;  // Re-export PReg, RegClass, PregSet, etc.

// Individual register constants (same as Phase 2)
pub const ZERO: PReg = PReg { hw: 0, class: RegClass::Int };
pub const RA: PReg = PReg { hw: 1, class: RegClass::Int };
// ... all others

// Register sets
pub const ARG_REGS: [PReg; 8] = [A0, A1, A2, A3, A4, A5, A6, A7];
pub const RET_REGS: [PReg; 4] = [A0, A1, A2, A3];
pub const CALLEE_SAVED_REGS: [PReg; 12] = [S0, S1, S2, S3, S4, S5, S6, S7, S8, S9, S10, S11];
pub const SPILL_TEMPS: [PReg; 2] = [T0, T1];

// PregSet constants (constructed via const fn)
pub const CALLER_SAVED: PregSet = PregSet::from_bits(/* ... */);
pub const CALLEE_SAVED: PregSet = PregSet::from_bits(/* ... */);
pub const RESERVED_ALWAYS: PregSet = PregSet::from_bits(/* ... */);
pub const ALLOCA_BASE: PregSet = PregSet::from_bits(/* ... */);

// ABI parameters
pub const STACK_ALIGNMENT: u32 = 16;
pub const SRET_THRESHOLD: usize = 2;
```

## lib.rs Update

Add the abi2 module to `lp-shader/lpvm-native/src/lib.rs`:

```rust
// ... existing modules ...

// New abi system (will replace abi.rs after transition)
pub mod abi2;

// Existing modules
pub mod debug_asm;
pub mod error;
// ... etc
```

## isa/mod.rs Update

Update `lp-shader/lpvm-native/src/isa/mod.rs` to expose abi2:

```rust
pub mod rv32;

// Re-export for convenience
pub use rv32::{abi, abi2};
```

## Validate

```bash
# Check it all compiles
cargo check -p lpvm-native

# Run all abi tests
cargo test -p lpvm-native abi

# Verify no overlap with existing abi module
cargo test -p lpvm-native 2>&1 | grep -E "(abi|abi2)" | head -20
```

All tests should pass. The abi2 module should be fully functional but not yet wired into the existing emission paths.
