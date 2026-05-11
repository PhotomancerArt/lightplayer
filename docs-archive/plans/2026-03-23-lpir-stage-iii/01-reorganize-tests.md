# Phase 1: Reorganize Test Files

## Scope

Split the existing `tests.rs` into focused submodules. Move validator negative
tests to `tests/validate.rs`, create `tests/interp.rs` with shared helpers,
and keep round-trip / sizing / smoke tests in the root `tests.rs`.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation details

### 1. Create `tests/interp.rs`

Start with helpers and the two existing interpreter tests moved from `tests.rs`:

```rust
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::interp::{ImportHandler, InterpError, Value, interpret, interpret_with_depth};
use crate::parse::parse_module;

fn run(ir: &str, func: &str, args: &[Value]) -> Vec<Value> {
    let module = parse_module(ir).unwrap_or_else(|e| panic!("parse: {e}"));
    interpret(&module, func, args, &mut NoImports).unwrap()
}

fn run_i32(ir: &str, func: &str, args: &[Value]) -> i32 {
    let out = run(ir, func, args);
    assert_eq!(out.len(), 1, "expected 1 return value, got {}", out.len());
    out[0].as_i32().expect("expected i32")
}

fn run_f32(ir: &str, func: &str, args: &[Value]) -> f32 {
    let out = run(ir, func, args);
    assert_eq!(out.len(), 1, "expected 1 return value, got {}", out.len());
    out[0].as_f32().expect("expected f32")
}

struct NoImports;

impl ImportHandler for NoImports {
    fn call(&mut self, _: &str, _: &str, _: &[Value]) -> Result<Vec<Value>, InterpError> {
        Err(InterpError::Import(String::from("no imports")))
    }
}

#[test]
fn interp_add() {
    let r = run_f32(
        "func @f(v0:f32, v1:f32) -> f32 {\n  v2:f32 = fadd v0, v1\n  return v2\n}\n",
        "f",
        &[Value::F32(1.0), Value::F32(2.0)],
    );
    assert!((r - 3.0).abs() < 1e-6);
}

#[test]
fn interp_error_display() {
    let e = InterpError::FunctionNotFound(String::from("nope"));
    assert!(e.to_string().contains("nope"));
}
```

### 2. Create `tests/validate.rs`

Move all `validate_err_*` and `validate_*` tests from `tests.rs` into this
file. Include the necessary imports:

```rust
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::builder::FunctionBuilder;
use crate::module::{ImportDecl, IrFunction, IrModule};
use crate::op::Op;
use crate::parse::parse_module;
use crate::types::{CalleeRef, IrType, SlotId, VReg, VRegRange};
use crate::validate::{validate_function, validate_module};
```

Move these tests (names unchanged):
- `validate_parsed_control_flow_examples`
- `validate_simple_add_passes`
- `validate_err_break_outside_loop`
- `validate_err_duplicate_import`
- `validate_err_two_entry`
- `validate_err_undefined_vreg`
- `validate_err_copy_type_mismatch`
- `validate_err_call_arity`
- `validate_err_callee_oob`
- `validate_err_continue_outside_loop`
- `validate_err_duplicate_func_name_parsed`
- `validate_err_duplicate_switch_case`
- `validate_err_return_value_type`
- `validate_err_vreg_pool_oob`
- `validate_err_slot_addr_oob`

### 3. Update `tests.rs`

- Add `mod interp;` and `mod validate;` declarations (alongside existing
  `mod all_ops_roundtrip;`).
- Remove the moved tests.
- Remove imports that are no longer used (`FunctionBuilder`, `ImportHandler`,
  `InterpError`, `Value`, `interpret`, `validate_function`, `validate_module`,
  `CalleeRef`, `VRegRange`, `ImportDecl`, `IrFunction`, `IrModule`, `Op`).
- Keep: round-trip tests, `op_enum_payload_reasonable_size`,
  `parse_error_*`, `parse_accepts_hex_iconst`, `round_trip_all_ops`.

### 4. Verify

After reorganization, all 38 existing tests must still pass with the same names.

## Validate

```
cargo test -p lpir
cargo check -p lpir
cargo +nightly fmt -- --check
```

All 38 tests pass, zero warnings.
