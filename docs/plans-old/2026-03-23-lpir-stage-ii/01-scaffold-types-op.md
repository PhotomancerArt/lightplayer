# Phase 1: Crate Scaffold + Core Types + Op Enum

## Scope

Set up the `lpir` crate, define all data types (`IrType`, `VReg`, `SlotId`,
`VRegRange`, `CalleeRef`, `Op`, `IrModule`, `IrFunction`, `ImportDecl`,
`SlotDecl`), and register the crate in the workspace. No logic — only data
structures. Everything must compile and basic construction tests must pass.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation details

### 1. Cargo.toml

```toml
[package]
name = "lpir"
version = "0.1.0"
edition = "2024"

[dependencies]
# nom + nom_locate added in Phase 3 (parser)

[features]
default = []
std = []
```

`#![no_std]` crate with `extern crate alloc`.

### 2. Workspace registration

Add `"lp-shader/lpir"` to `[workspace].members` in the root `Cargo.toml`.
Also add it to `[workspace].default-members` if appropriate.

### 3. src/lib.rs

```rust
#![no_std]

extern crate alloc;

pub mod types;
pub mod op;
pub mod module;
```

Re-export key types at the crate root for convenience:

```rust
pub use types::{IrType, VReg, SlotId, VRegRange, CalleeRef};
pub use op::Op;
pub use module::{IrModule, IrFunction, ImportDecl, SlotDecl};
```

### 4. src/types.rs

```rust
use core::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum IrType {
    F32,
    I32,
}

impl fmt::Display for IrType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IrType::F32 => write!(f, "f32"),
            IrType::I32 => write!(f, "i32"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct VReg(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SlotId(pub u32);

#[derive(Clone, Copy, Debug)]
pub struct VRegRange {
    pub start: u32,
    pub count: u16,
}

impl VRegRange {
    pub const EMPTY: VRegRange = VRegRange { start: 0, count: 0 };
}

/// Index into the module's combined callee table.
///
/// Layout: imports occupy indices 0..imports.len(), then local functions
/// occupy imports.len()..imports.len()+functions.len().
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CalleeRef(pub u32);
```

Add `Display` impls for `VReg` (`v0`, `v1`, ...) and `SlotId` (`ss0`, `ss1`,
...) — these will be used by the printer.

### 5. src/op.rs

The full `Op` enum with all ~50 variants as listed in the design document.
Group with comments. Derive `Debug`, `Clone`. Consider `PartialEq` for test
assertions (note: `f32` in `FconstF32` makes `PartialEq` require care —
implement manually or use a wrapper for NaN-aware comparison in tests).

### 6. src/module.rs

```rust
use alloc::string::String;
use alloc::vec::Vec;
use crate::types::*;
use crate::op::Op;

pub struct IrModule {
    pub imports: Vec<ImportDecl>,
    pub functions: Vec<IrFunction>,
}

pub struct ImportDecl {
    pub module_name: String,
    pub func_name: String,
    pub param_types: Vec<IrType>,
    pub return_types: Vec<IrType>,
}

pub struct SlotDecl {
    pub size: u32,
}

pub struct IrFunction {
    pub name: String,
    pub is_entry: bool,
    pub param_count: u16,
    pub return_types: Vec<IrType>,
    pub vreg_types: Vec<IrType>,
    pub slots: Vec<SlotDecl>,
    pub body: Vec<Op>,
    pub vreg_pool: Vec<VReg>,
}
```

Add `IrModule::callee_ref_for_import(idx)` and
`IrModule::callee_ref_for_function(idx)` helper methods for constructing
`CalleeRef` values. Add `IrFunction::pool_slice(&self, range: VRegRange) ->
&[VReg]` helper for reading from the vreg pool.

### 7. Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construct_basic_op() {
        let op = Op::Fadd {
            dst: VReg(2),
            lhs: VReg(0),
            rhs: VReg(1),
        };
        // Just verify it compiles and can be matched
        assert!(matches!(op, Op::Fadd { .. }));
    }

    #[test]
    fn construct_ir_function() {
        let func = IrFunction {
            name: String::from("test"),
            is_entry: false,
            param_count: 2,
            return_types: vec![IrType::F32],
            vreg_types: vec![IrType::F32, IrType::F32, IrType::F32],
            slots: vec![],
            body: vec![
                Op::Fadd {
                    dst: VReg(2),
                    lhs: VReg(0),
                    rhs: VReg(1),
                },
                Op::Return {
                    values: VRegRange { start: 0, count: 1 },
                },
            ],
            vreg_pool: vec![VReg(2)],
        };
        assert_eq!(func.body.len(), 2);
        assert_eq!(func.vreg_types.len(), 3);
    }

    #[test]
    fn op_enum_size() {
        // Ensure Op stays compact — should be ≤ 20 bytes
        assert!(core::mem::size_of::<Op>() <= 20,
            "Op size {} exceeds 20 bytes", core::mem::size_of::<Op>());
    }

    #[test]
    fn vreg_range_empty() {
        let r = VRegRange::EMPTY;
        assert_eq!(r.count, 0);
    }
}
```

The `op_enum_size` test enforces the compact layout constraint.

## Validate

```
cargo check -p lpir
cargo test -p lpir
cargo +nightly fmt -- --check
```

Fix any warnings (unused imports, dead code for types not yet used by later
phases — suppress with `#[allow(dead_code)]` only if the type will be used in
the next phase).
