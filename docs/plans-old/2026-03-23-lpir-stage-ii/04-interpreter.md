# Phase 4: Interpreter

## Scope

Implement a complete LPIR interpreter that executes an `IrFunction` within an
`IrModule` with concrete inputs and returns results. The interpreter covers all
op types, control flow, memory operations, local function calls, and import
calls (via a pluggable `ImportHandler` trait). With the parser available from
Phase 3, tests can use readable text-format IR strings.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation details

### 1. src/interp.rs — Public API

```rust
use alloc::string::String;
use alloc::vec::Vec;
use crate::module::*;
use crate::types::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    F32(f32),
    I32(i32),
}

impl Value {
    pub fn as_f32(self) -> f32;   // panics if I32
    pub fn as_i32(self) -> i32;   // panics if F32
    pub fn is_truthy(self) -> bool { self.as_i32() != 0 }
}

#[derive(Debug)]
pub enum InterpError {
    FunctionNotFound(String),
    TypeMismatch { expected: IrType, got: IrType },
    ArityMismatch { expected: usize, got: usize },
    StackOverflow,
    ImportError(String),
    InvalidMemoryAccess,
}

pub trait ImportHandler {
    fn call(
        &mut self,
        module_name: &str,
        func_name: &str,
        args: &[Value],
    ) -> Result<Vec<Value>, String>;
}

pub fn interpret(
    module: &IrModule,
    func_name: &str,
    args: &[Value],
    imports: &mut dyn ImportHandler,
) -> Result<Vec<Value>, InterpError>;
```

### 2. Interpreter internals

#### Call frame

```rust
struct Frame {
    func_idx: usize,
    regs: Vec<Value>,         // indexed by VReg.0
    slot_mem: Vec<u8>,        // flattened slot memory
    slot_offsets: Vec<u32>,   // byte offset of each slot in slot_mem
    pc: usize,                // program counter into body
}
```

Slot memory: compute `slot_offsets` from cumulative `SlotDecl.size` values.
Allocate `slot_mem` with total size. All slots start with undefined bytes (in
practice, zero-initialized for determinism in the interpreter).

#### Control flow

The interpreter maintains a `loop_stack: Vec<LoopCtx>` per frame:

```rust
struct LoopCtx {
    start_pc: usize,   // index of LoopStart
    end_pc: usize,     // index of End (from LoopStart.end_offset)
}
```

- `LoopStart`: push a `LoopCtx`.
- `End` where the top of `loop_stack` matches: pop and re-enter (jump to
  `start_pc + 1`). Wait — actually `End` for a loop should just pop the loop
  context and fall through (the loop body must contain an explicit `break` or
  `continue` or `br_if_not` to exit or repeat). Let me reconsider.

The LPIR spec says `loop` is "infinite repetition of body until `break` or
`br_if_not` exits." So the `End` of a loop means "jump back to start" (like
WASM `loop`).

Corrected control flow:

- **`IfStart`**: evaluate `cond`. If false, jump to `else_offset`. If true,
  continue (enter then body).
- **`Else`**: (reached at end of then body) jump to `end_offset` of the
  enclosing `IfStart`.
- **`End` closing an `if`**: fall through.
- **`LoopStart`**: push `LoopCtx { start_pc, end_pc }`. Continue into body.
- **`End` closing a `loop`**: jump back to `start_pc + 1` (re-enter body).
  This implements infinite looping; the body must `break` to exit.
- **`Break`**: jump to `loop_stack.last().end_pc + 1` (past the `End`). Pop
  the `LoopCtx`.
- **`Continue`**: jump to `loop_stack.last().start_pc + 1` (re-enter body
  from top). Do NOT pop the `LoopCtx`.
- **`BrIfNot { cond }`**: if `cond` is zero, behave like `Break`. Otherwise
  fall through.
- **`SwitchStart`**: evaluate selector. Scan `CaseStart` ops sequentially
  (following `end_offset` links to skip case bodies). If a match is found,
  enter that case body. If no match and `DefaultStart` exists, enter default.
  If no match and no default, jump to `end_offset + 1`.
- **`End` closing a `switch`**: fall through.

The interpreter needs a nesting stack to know what each `End` closes. This can
be done by tracking block types:

```rust
enum BlockType { If, Loop, Switch, Case }
struct BlockCtx { kind: BlockType, start_pc: usize, end_pc: usize }
```

On `IfStart`, push `BlockCtx { kind: If, ..., end_pc }`.
On `LoopStart`, push both a `BlockCtx` and a `LoopCtx`.
On `End`, pop `BlockCtx` and handle based on `kind`.

#### Op execution

For each non-control-flow op, execute the semantics from the spec:

**Float arithmetic**: native `f32` operations.
```rust
Op::Fadd { dst, lhs, rhs } => {
    let a = regs[lhs].as_f32();
    let b = regs[rhs].as_f32();
    regs[dst] = Value::F32(a + b);
}
```

**Integer arithmetic**: wrapping ops via Rust's `wrapping_add`, etc.
```rust
Op::Iadd { dst, lhs, rhs } => {
    let a = regs[lhs].as_i32();
    let b = regs[rhs].as_i32();
    regs[dst] = Value::I32(a.wrapping_add(b));
}
```

**Integer division by zero**: result is `0` (spec: non-trapping).
```rust
Op::IdivS { dst, lhs, rhs } => {
    let a = regs[lhs].as_i32();
    let b = regs[rhs].as_i32();
    regs[dst] = Value::I32(if b == 0 { 0 } else { a.wrapping_div(b) });
}
```

**Shifts**: mask amount to 5 bits.
```rust
Op::Ishl { dst, lhs, rhs } => {
    let a = regs[lhs].as_i32();
    let b = regs[rhs].as_i32();
    regs[dst] = Value::I32(a.wrapping_shl((b & 31) as u32));
}
```

**Float comparisons**: produce `i32` 0 or 1. NaN handling per spec.
```rust
Op::Flt { dst, lhs, rhs } => {
    let a = regs[lhs].as_f32();
    let b = regs[rhs].as_f32();
    regs[dst] = Value::I32(if a < b { 1 } else { 0 });
    // f32 < returns false if either is NaN, which is correct
}
```

**`Fne`**: special — true if NaN or unequal.
```rust
Op::Fne { dst, lhs, rhs } => {
    let a = regs[lhs].as_f32();
    let b = regs[rhs].as_f32();
    regs[dst] = Value::I32(if a != b || a.is_nan() || b.is_nan() { 1 } else { 0 });
    // Actually: a != b already returns true if either is NaN in IEEE 754
    // So just: Value::I32(if a != b { 1 } else { 0 })
}
```

**Saturating casts**: `ftoi_sat_s`, `ftoi_sat_u` — use Rust's `as` cast which
is saturating on stable Rust (since 1.45).

**Select**:
```rust
Op::Select { dst, cond, if_true, if_false } => {
    regs[dst] = if regs[cond].is_truthy() { regs[if_true] } else { regs[if_false] };
}
```

**Memory ops**:
- `SlotAddr { dst, slot }` → `regs[dst] = Value::I32(slot_offsets[slot])`.
  But addresses are into `slot_mem` — the interpreter uses the offset directly
  as an index into the byte array.
- `Load { dst, base, offset }` → read 4 bytes from `slot_mem[addr..addr+4]`,
  interpret as `f32` or `i32` based on `vreg_types[dst]`.
- `Store { base, offset, value }` → write 4 bytes to `slot_mem[addr..addr+4]`.
- `Memcpy { dst_addr, src_addr, size }` → `slot_mem` copy.

For loads/stores, use `from_le_bytes` / `to_le_bytes` (little-endian per spec).

**Call**:
- Resolve `CalleeRef` — if it's an import index, call `ImportHandler`. If it's
  a local function index, push a new frame and recurse.
- Collect args from `vreg_pool[args.start..args.start+args.count]`, read their
  values from regs.
- After call returns, write results to `vreg_pool[results.start..]` regs.

**Return**:
- Collect return values from pool range, return them to the caller frame.

#### Stack depth limit

The interpreter should enforce a configurable maximum call depth (e.g., 256)
to prevent stack overflow from unbounded recursion. Return `InterpError::StackOverflow`
when exceeded.

### 3. Tests

With the parser, tests can use text-format IR:

```rust
fn run(ir: &str, func: &str, args: &[Value]) -> Vec<Value> {
    let module = parse_module(ir).unwrap();
    interpret(&module, func, args, &mut NoImports).unwrap()
}

struct NoImports;
impl ImportHandler for NoImports {
    fn call(&mut self, m: &str, n: &str, _: &[Value]) -> Result<Vec<Value>, String> {
        Err(format!("no handler for @{m}::{n}"))
    }
}
```

#### Arithmetic tests

```rust
#[test]
fn interp_fadd() {
    let result = run("func @f(v0:f32, v1:f32) -> f32 {\n  v2:f32 = fadd v0, v1\n  return v2\n}\n",
        "f", &[Value::F32(1.0), Value::F32(2.0)]);
    assert_eq!(result, vec![Value::F32(3.0)]);
}
```

Similar tests for: `fsub`, `fmul`, `fdiv`, `fneg`, `iadd`, `isub`, `imul`,
`idiv_s`, `idiv_u`, `irem_s`, `irem_u`, `ineg`.

#### Comparison tests

Test each comparison op with a few representative inputs including edge cases
(equal, less, greater, and NaN for floats).

#### Control flow tests

- `interp_if_true` / `interp_if_false` — conditional paths
- `interp_if_else` — the `@max` example
- `interp_loop_sum` — the `@sum_to_n` example
- `interp_nested_loops` — the `@nested` example
- `interp_switch` — the `@dispatch` example with various selector values
- `interp_switch_default` — selector hits default
- `interp_switch_no_default` — selector matches no case, no default → skip
- `interp_br_if_not` — loop exit condition
- `interp_early_return` — return from inside if

#### Memory tests

- `interp_slot_load_store` — store a value, load it back
- `interp_memcpy` — copy between slots
- `interp_dynamic_index` — the `@arr_dyn` example
- `interp_out_param` — callee writes through pointer

#### Call tests

- `interp_local_call` — call a helper function
- `interp_import_call` — mock `@std.math::fsin` via ImportHandler
- `interp_multi_return` — function returns multiple values
- `interp_recursion` — recursive factorial, verify result
- `interp_recursion_depth_limit` — unbounded recursion hits stack limit

#### Immediate and cast tests

- `interp_iadd_imm` — immediate add
- `interp_ftoi_sat_s` — float to int, saturating
- `interp_itof_s` — int to float
- `interp_select` — select between two values

## Validate

```
cargo check -p lpir
cargo test -p lpir
cargo +nightly fmt -- --check
```
