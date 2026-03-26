# Phase 1: Restructure emit.rs into emit/ module

## Scope

Convert `emit.rs` into an `emit/` module directory. Move existing scalar ops
into `emit/scalar.rs`, set up `emit/mod.rs` with the dispatch loop, `EmitCtx`,
`CtrlFrame` enum (empty for now), and shared helpers. Stub `emit/control.rs`,
`emit/memory.rs`, `emit/call.rs`. Existing test passes unchanged.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Create `emit/` directory structure

Delete `src/emit.rs`, create:

- `src/emit/mod.rs`
- `src/emit/scalar.rs`
- `src/emit/control.rs` (stub)
- `src/emit/memory.rs` (stub)
- `src/emit/call.rs` (stub)

### 2. `emit/mod.rs` — entry point, types, dispatch

Public items:
- `signature_for_ir_func` (moved from old emit.rs, unchanged)
- `translate_function` (refactored — see below)

Internal types:

```rust
use cranelift_codegen::ir::{FuncRef, StackSlot, Value};
use cranelift_frontend::{FunctionBuilder, Variable};
use lpir::module::{IrFunction, IrModule};

use crate::error::CompileError;

pub(crate) struct EmitCtx<'a> {
    pub func_refs: &'a [FuncRef],
    pub slots: &'a [StackSlot],
    pub ir: &'a IrModule,
}

enum CtrlFrame {
    // populated in Phase 2
}
```

Shared helpers (moved from old emit.rs):

```rust
pub(crate) fn ir_type(t: IrType) -> types::Type { ... }
pub(crate) fn use_v(builder: &mut FunctionBuilder, vars: &[Variable], v: VReg) -> Value { ... }
pub(crate) fn def_v(builder: &mut FunctionBuilder, vars: &[Variable], v: VReg, val: Value) { ... }
pub(crate) fn def_v_expr(...) { ... }
pub(crate) fn bool_to_i32(builder: &mut FunctionBuilder, b: Value) -> Value { ... }
```

The `translate_function` signature changes:

```rust
pub fn translate_function(
    func: &IrFunction,
    builder: &mut FunctionBuilder,
    ctx: &EmitCtx,
) -> Result<(), CompileError> {
    // ... var declaration, param binding (same as before) ...

    let mut ctrl_stack: Vec<CtrlFrame> = Vec::new();

    for op in &func.body {
        match op {
            // Scalar ops → scalar::emit_scalar(op, func, builder, &vars)?
            // Control flow → control::emit_control(op, builder, &vars, &mut ctrl_stack)?
            // Memory → memory::emit_memory(op, func, builder, &vars, ctx)?
            // Call/Return → call::emit_call(op, func, builder, &vars, ctx)?
            _ => return Err(CompileError::unsupported(...))
        }
    }

    builder.seal_all_blocks();
    Ok(())
}
```

For now, control/memory/call arms fall through to the `_ =>` error (those
submodules are stubs). Scalar ops and Return dispatch to their submodules.

Remove the `uses_memory()` guard.

Remove the `seal_block(entry)` call from `jit_module.rs` — sealing is now
handled by `seal_all_blocks()` at the end of `translate_function`.

### 3. `emit/scalar.rs` — mechanical 1:1 ops

Move all existing match arms from old emit.rs. Export a single dispatch
function:

```rust
pub(crate) fn emit_scalar(
    op: &Op,
    func: &IrFunction,
    builder: &mut FunctionBuilder,
    vars: &[Variable],
) -> Result<bool, CompileError>
```

Returns `Ok(true)` if the op was handled, `Ok(false)` if not (caller tries
other submodules). This avoids duplicating the op list in mod.rs.

Ops handled: all float arithmetic, integer arithmetic, immediate ops,
float comparisons, integer comparisons, constants, casts, Select, Copy.

### 4. `emit/call.rs` — Return (for now)

Move `Op::Return` handling here. Call handling added in Phase 5.

```rust
pub(crate) fn emit_call(
    op: &Op,
    func: &IrFunction,
    builder: &mut FunctionBuilder,
    vars: &[Variable],
    ctx: &EmitCtx,
) -> Result<bool, CompileError>
```

### 5. `emit/control.rs` and `emit/memory.rs` — stubs

```rust
// control.rs
pub(crate) fn emit_control(...) -> Result<bool, CompileError> {
    Ok(false) // not yet implemented
}

// memory.rs
pub(crate) fn emit_memory(...) -> Result<bool, CompileError> {
    Ok(false)
}
```

### 6. Update `jit_module.rs`

- Remove `uses_memory()` guard
- Remove `builder.seal_block(entry)` (translate_function handles sealing)
- Add `EmitCtx` construction (empty func_refs and slots for now):

```rust
let ctx = emit::EmitCtx {
    func_refs: &[],
    slots: &[],
    ir,
};
translate_function(f, &mut builder, &ctx)?;
```

FuncRef and StackSlot population come in Phases 4 and 5.

### 7. Update `lib.rs`

Change `mod emit;` — this now refers to the directory module. No other changes
needed if re-exports stay the same.

## Validate

```
cargo check -p lpir-cranelift
cargo test -p lpir-cranelift
```

The existing `jit_linear_fadd_f32` test must still pass. No new tests in this
phase — it's a pure restructure.
