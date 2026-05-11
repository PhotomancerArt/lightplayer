# Phase 2: FunctionBuilder + Text Format Printer

## Scope

Implement the builder API for constructing `IrFunction` and `IrModule` values,
and the text format printer that serializes `IrModule` to a string matching the
spec format. These are paired because the printer is the primary way to verify
builder output.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation details

### 1. src/builder.rs — FunctionBuilder

The builder constructs a flat op stream with stack-based control flow offset
patching.

```rust
use alloc::string::String;
use alloc::vec::Vec;
use crate::types::*;
use crate::op::Op;
use crate::module::*;

pub struct FunctionBuilder {
    name: String,
    is_entry: bool,
    return_types: Vec<IrType>,
    vreg_types: Vec<IrType>,
    slots: Vec<SlotDecl>,
    body: Vec<Op>,
    vreg_pool: Vec<VReg>,
    block_stack: Vec<BlockEntry>,
}

enum BlockEntry {
    If { start_idx: usize },
    Else { if_start_idx: usize, else_idx: usize },
    Loop { start_idx: usize },
    Switch { start_idx: usize, case_indices: Vec<usize> },
}
```

#### Methods

**VReg / slot allocation:**
- `add_param(ty: IrType) -> VReg` — allocates the next VReg as a parameter.
  Must be called before any ops. Increments `param_count`.
- `alloc_vreg(ty: IrType) -> VReg` — allocates a non-parameter VReg.
- `alloc_slot(size: u32) -> SlotId` — appends a `SlotDecl`.

**Scalar / memory ops:**
- `push(op: Op)` — appends an op to the body. Used for all non-control-flow
  ops (arithmetic, memory, constants, etc.) and for `Break`, `Continue`,
  `BrIfNot`.

**Control flow (stack-based):**

`push_if(cond: VReg)`:
- Push `IfStart { cond, else_offset: 0, end_offset: 0 }` (placeholder offsets).
- Push `BlockEntry::If { start_idx }` onto `block_stack`.

`push_else()`:
- Pop `BlockEntry::If { start_idx }` from stack.
- Push `Else` op at current index.
- Patch `body[start_idx]` (`IfStart.else_offset`) to point to the `Else` index.
- Push `BlockEntry::Else { if_start_idx, else_idx }` onto stack.

`end_if()`:
- Pop from stack. If `BlockEntry::If`, there was no else — patch both
  `else_offset` and `end_offset` to the `End` index. If `BlockEntry::Else`,
  patch `IfStart.end_offset` to the `End` index.
- Push `End` op.

`push_loop()`:
- Push `LoopStart { end_offset: 0 }`.
- Push `BlockEntry::Loop { start_idx }`.

`end_loop()`:
- Pop `BlockEntry::Loop`. Patch `LoopStart.end_offset` to the `End` index.
- Push `End`.

`push_switch(selector: VReg)`:
- Push `SwitchStart { selector, end_offset: 0 }`.
- Push `BlockEntry::Switch { start_idx, case_indices: vec![] }`.

`push_case(value: i32)`:
- Peek at top of stack — must be `Switch`.
- If there's a previous `CaseStart` or `DefaultStart` without an `end_offset`
  patched, patch it to the current index.
- Push `CaseStart { value, end_offset: 0 }`.
- Record the index in the `Switch` entry's `case_indices`.

`push_default()`:
- Same as `push_case` but pushes `DefaultStart { end_offset: 0 }`.

`end_switch()`:
- Pop `BlockEntry::Switch`. Patch any unpatched last case/default `end_offset`
  to the `End` index. Patch `SwitchStart.end_offset` to the `End` index.
- Push `End`.

**Call / return (vreg_pool):**

`push_call(callee: CalleeRef, args: &[VReg], results: &[VReg])`:
- Record `start = vreg_pool.len()`.
- Extend `vreg_pool` with `args`.
- Push `Op::Call { callee, args: VRegRange { start, count: args.len() }, results: ... }`.
- Extend `vreg_pool` with `results` (similarly).

Actually, args and results need separate ranges:
- `args_start = pool.len(); pool.extend(args);`
- `results_start = pool.len(); pool.extend(results);`
- Push `Op::Call { callee, args: VRegRange { start: args_start, count: args.len() }, results: VRegRange { start: results_start, count: results.len() } }`.

`push_return(values: &[VReg])`:
- `start = pool.len(); pool.extend(values);`
- Push `Op::Return { values: VRegRange { start, count: values.len() } }`.

**Finish:**

`finish(self) -> IrFunction` — consumes the builder, asserts `block_stack`
is empty (all blocks closed), returns the `IrFunction`.

#### ModuleBuilder

```rust
pub struct ModuleBuilder {
    imports: Vec<ImportDecl>,
    functions: Vec<IrFunction>,
}

impl ModuleBuilder {
    pub fn new() -> Self;
    pub fn add_import(&mut self, decl: ImportDecl) -> CalleeRef;
    pub fn add_function(&mut self, func: IrFunction) -> CalleeRef;
    pub fn finish(self) -> IrModule;
}
```

`add_import` returns `CalleeRef(imports.len() - 1)` (index of the just-added
import). `add_function` returns `CalleeRef(imports.len() + functions.len() - 1)`.

### 2. src/print.rs — Text Format Printer

`print_module(module: &IrModule) -> String` — produces the complete text
representation.

The printer walks the module and produces output matching the spec text format:

```
import @std.math::fsin(f32) -> f32

func @helper(v0:f32, v1:f32) -> f32 {
  v2:f32 = fadd v0, v1
  return v2
}

entry func @main(v0:f32) -> f32 {
  v1:f32 = call @helper(v0, v0)
  return v1
}
```

#### Printing rules

- **Imports**: `import @{module}::{name}({param_types}) [-> {return_types}]`
- **Functions**: `[entry ]func @{name}({params}) [-> {return_type}] { ... }`
- **Params**: `v0:f32, v1:i32` (VReg with type annotation)
- **Return type**: single type bare, multiple types in parens `(f32, f32, f32)`
- **Slots**: `  slot ss0, 64` (indented, before body ops)
- **Ops**: indented by nesting depth (2 spaces per level)
- **VReg definitions**: type annotation on first occurrence (`v2:f32 = fadd ...`)
- **VReg uses**: no type annotation (`v0`, `v1`)

The printer needs to track which VRegs have already been defined (first
occurrence gets `:type` suffix). Since VRegs are dense and parameters are
defined in the signature, track a `defined: Vec<bool>` indexed by VReg. Params
are pre-marked as defined.

#### Control flow printing

The printer walks the flat op stream and adjusts indentation:

- `IfStart` → print `if v_cond {`, increase indent
- `Else` → decrease indent, print `} else {`, increase indent
- `End` → decrease indent, print `}`
- `LoopStart` → print `loop {`, increase indent
- `SwitchStart` → print `switch v_sel {`, increase indent
- `CaseStart` → print `case N {`, increase indent
- `DefaultStart` → print `default {`, increase indent
- On `CaseStart`/`DefaultStart`/`End` following a case body, close the previous
  case (`}`) first

Actually, the printer needs a small state machine to know when to close case
bodies. The skip-offsets help: when `pc == some_case.end_offset`, close that
case. Alternatively, track open case blocks with a small stack.

Simpler approach: maintain an indentation stack. On `CaseStart`/`DefaultStart`,
if the previous stack entry is a case, pop and close it first. On `End`, pop
and close.

#### Op printing

Each op variant maps to its text form. Examples:
- `Fadd { dst, lhs, rhs }` → `v{dst}:f32 = fadd v{lhs}, v{rhs}` (with type
  if first definition)
- `IconstI32 { dst, value }` → `v{dst}:i32 = iconst.i32 {value}`
- `Call { callee, args, results }` → resolve callee name from module, print
  `[results =] call @name(args)`
- `Return { values }` → `return [v0, v1, ...]`

### 3. Tests

```rust
#[test]
fn build_and_print_add() {
    // Build: func @add(v0:f32, v1:f32) -> f32 { v2 = fadd v0, v1; return v2 }
    let mut fb = FunctionBuilder::new("add", &[IrType::F32]);
    let v0 = fb.add_param(IrType::F32);
    let v1 = fb.add_param(IrType::F32);
    let v2 = fb.alloc_vreg(IrType::F32);
    fb.push(Op::Fadd { dst: v2, lhs: v0, rhs: v1 });
    fb.push_return(&[v2]);
    let func = fb.finish();

    let mut mb = ModuleBuilder::new();
    mb.add_function(func);
    let module = mb.finish();

    let text = print_module(&module);
    let expected = "\
func @add(v0:f32, v1:f32) -> f32 {
  v2:f32 = fadd v0, v1
  return v2
}
";
    assert_eq!(text, expected);
}

#[test]
fn build_and_print_if_else() {
    // Build the @max example from the spec
    // ...
}

#[test]
fn build_and_print_loop() {
    // Build the @sum_to_n example from the spec
    // ...
}

#[test]
fn build_and_print_switch() {
    // Build the @dispatch example from the spec
    // ...
}

#[test]
fn build_and_print_call_import() {
    // Build a module with an import and a call to it
    // ...
}

#[test]
fn build_and_print_multi_return() {
    // Build a function returning multiple values
    // ...
}

#[test]
fn build_and_print_memory_ops() {
    // Build a function with slots, slot_addr, load, store, memcpy
    // ...
}

#[test]
fn build_and_print_entry_func() {
    // Verify "entry func @main(...)" output
    // ...
}
```

Each test builds IR using the builder, prints it, and compares to expected
text matching the spec examples from `docs/lpir/04-control-flow.md` and others.

## Validate

```
cargo check -p lpir
cargo test -p lpir
cargo +nightly fmt -- --check
```
