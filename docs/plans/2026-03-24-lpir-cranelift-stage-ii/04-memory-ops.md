# Phase 4: Memory Ops

## Scope

Implement `SlotAddr`, `Load`, `Store`, `Memcpy` in `emit/memory.rs`. Wire
up stack slot allocation in `jit_module.rs`. Add tests.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Stack slot allocation in `jit_module.rs`

Before calling `translate_function`, allocate Cranelift stack slots from
the LPIR `SlotDecl`s:

```rust
use cranelift_codegen::ir::{StackSlotData, StackSlotKind};

let slots: Vec<StackSlot> = func.slots.iter().map(|sd| {
    builder.func.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        sd.size,
        0, // alignment — let Cranelift choose
    ))
}).collect();
```

Note: slots must be created on `builder.func` (which is `&mut ctx.func`)
*before* the `FunctionBuilder` is created, or through the builder. Since
`create_sized_stack_slot` is on `Function`, not `FunctionBuilder`, we
can call it through `builder.func`:

```rust
let mut builder = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
// ... create entry block, append params ...

let slots: Vec<StackSlot> = func.slots.iter().map(|sd| {
    builder.func.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        sd.size,
        0,
    ))
}).collect();
```

Pass `&slots` to `EmitCtx`.

### 2. `emit/memory.rs`

```rust
pub(crate) fn emit_memory(
    op: &Op,
    func: &IrFunction,
    builder: &mut FunctionBuilder,
    vars: &[Variable],
    ctx: &EmitCtx,
) -> Result<bool, CompileError>
```

#### SlotAddr

Get the address of a stack slot. The pointer type comes from the ISA
(I64 on x86-64, I32 on RV32). For Stage II (host JIT only), we can use
`builder.func.dfg.value_type(...)` or just hardcode the pointer type.
Better: pass `pointer_type` in `EmitCtx`.

```rust
Op::SlotAddr { dst, slot } => {
    let ss = ctx.slots[slot.0 as usize];
    let addr = builder.ins().stack_addr(ctx.pointer_type, ss, 0);
    def_v(builder, vars, *dst, addr);
}
```

Wait — LPIR `SlotAddr.dst` has type `I32` (it's an abstract address in LPIR).
But Cranelift `stack_addr` produces `pointer_type` which is `I64` on x86-64.
We need to handle this mismatch.

Options:
- (a) Truncate the Cranelift pointer to I32 with `ireduce`. Loses high bits
  but LPIR only uses addresses for Load/Store which we control.
- (b) Declare the VReg as the native pointer type instead of I32. Requires
  overriding `ir_type` for address-typed VRegs.
- (c) Use I64 internally, `ireduce` when LPIR wants I32.

Best approach: (a) — the address only flows into Load/Store/Memcpy which
we control. In those ops, `uextend` the I32 back to pointer_type before
using as a base address. This keeps LPIR types consistent.

```rust
Op::SlotAddr { dst, slot } => {
    let ss = ctx.slots[slot.0 as usize];
    let addr = builder.ins().stack_addr(ctx.pointer_type, ss, 0);
    // LPIR addresses are I32; truncate if pointer is wider
    let val = if ctx.pointer_type == types::I32 {
        addr
    } else {
        builder.ins().ireduce(types::I32, addr)
    };
    def_v(builder, vars, *dst, val);
}
```

#### Load

```rust
Op::Load { dst, base, offset } => {
    let base_val = use_v(builder, vars, *base);
    let addr = widen_to_ptr(builder, base_val, ctx.pointer_type);
    let ty = ir_type(func.vreg_types[dst.0 as usize]);
    let val = builder.ins().load(ty, MemFlags::new(), addr, *offset as i32);
    def_v(builder, vars, *dst, val);
}
```

#### Store

```rust
Op::Store { base, offset, value } => {
    let base_val = use_v(builder, vars, *base);
    let addr = widen_to_ptr(builder, base_val, ctx.pointer_type);
    let val = use_v(builder, vars, *value);
    builder.ins().store(MemFlags::new(), val, addr, *offset as i32);
}
```

#### Memcpy

Cranelift doesn't have a single memcpy instruction for stack-to-stack copies.
For small known sizes (LPIR `Memcpy.size` is a compile-time constant), emit
a sequence of load/store pairs:

```rust
Op::Memcpy { dst_addr, src_addr, size } => {
    let dst_val = use_v(builder, vars, *dst_addr);
    let src_val = use_v(builder, vars, *src_addr);
    let dst_ptr = widen_to_ptr(builder, dst_val, ctx.pointer_type);
    let src_ptr = widen_to_ptr(builder, src_val, ctx.pointer_type);

    // Copy in 4-byte (I32) chunks
    let mut off = 0i32;
    while (off as u32) < *size {
        let chunk = builder.ins().load(types::I32, MemFlags::new(), src_ptr, off);
        builder.ins().store(MemFlags::new(), chunk, dst_ptr, off);
        off += 4;
    }
}
```

This works because LPIR slot sizes are always multiples of 4 (they hold
scalar I32/F32 values).

### 3. Helper functions (bottom of memory.rs)

```rust
fn widen_to_ptr(builder: &mut FunctionBuilder, val: Value, ptr_type: types::Type) -> Value {
    if ptr_type == types::I32 {
        val
    } else {
        builder.ins().uextend(ptr_type, val)
    }
}
```

### 4. Update `EmitCtx`

Add `pointer_type: types::Type` field. Set it in `jit_module.rs` from
`jit_module.isa().pointer_type()`.

### 5. Tests

**`test_slot_load_store`** — round-trip through a stack slot:
```
func @roundtrip(v0:f32) -> f32 {
  slot ss0, 4
  v1:i32 = slot_addr ss0
  store v1, 0, v0
  v2:f32 = load v1, 0
  return v2
}
```
Verify `roundtrip(42.0) == 42.0`.

**`test_slot_multiple_values`** — store two values, load them back:
```
func @swap(v0:f32, v1:f32) -> f32, f32 {
  slot ss0, 8
  v2:i32 = slot_addr ss0
  store v2, 0, v0
  store v2, 4, v1
  v3:f32 = load v2, 4
  v4:f32 = load v2, 0
  return v3, v4
}
```
Verify `swap(1.0, 2.0) == (2.0, 1.0)`.

**`test_memcpy`** — copy slot contents:
```
func @copy_slot(v0:f32, v1:f32) -> f32, f32 {
  slot ss0, 8
  slot ss1, 8
  v2:i32 = slot_addr ss0
  store v2, 0, v0
  store v2, 4, v1
  v3:i32 = slot_addr ss1
  memcpy v3, v2, 8
  v4:f32 = load v3, 0
  v5:f32 = load v3, 4
  return v4, v5
}
```
Verify `copy_slot(3.0, 7.0) == (3.0, 7.0)`.

## Validate

```
cargo check -p lpir-cranelift
cargo test -p lpir-cranelift
```
