# Phase 1: Define NumericStrategy and FloatStrategy

## New file: `frontend/codegen/numeric.rs`

Create the `NumericStrategy` trait and `FloatStrategy` implementation.
Use the enum approach (recommended in the design doc) to avoid generic
parameter propagation.

```rust
use cranelift_codegen::ir::{types, condcodes::FloatCC, Signature, Type, Value};
use cranelift_frontend::FunctionBuilder;

pub enum NumericMode {
    Float(FloatStrategy),
}

pub struct FloatStrategy;
```

### Trait methods

Group into categories. Each method on `NumericMode` dispatches via match.

**Types:**
- `scalar_type() -> Type` — returns `types::F32`

**Constants:**
- `emit_const(val: f32, builder: &mut FunctionBuilder) -> Value`
  — emits `builder.ins().f32const(val)`

**Arithmetic:**
- `emit_add(a, b, builder) -> Value` — `builder.ins().fadd(a, b)`
- `emit_sub(a, b, builder) -> Value` — `builder.ins().fsub(a, b)`
- `emit_mul(a, b, builder) -> Value` — `builder.ins().fmul(a, b)`
- `emit_div(a, b, builder) -> Value` — `builder.ins().fdiv(a, b)`
- `emit_neg(a, builder) -> Value` — `builder.ins().fneg(a)`
- `emit_abs(a, builder) -> Value` — `builder.ins().fabs(a)`

**Comparison:**
- `emit_cmp(cc: FloatCC, a, b, builder) -> Value` — `builder.ins().fcmp(cc, a, b)`
- `emit_min(a, b, builder) -> Value` — `builder.ins().fmin(a, b)`
- `emit_max(a, b, builder) -> Value` — `builder.ins().fmax(a, b)`

**Rounding:**
- `emit_floor(a, builder) -> Value` — `builder.ins().floor(a)`
- `emit_ceil(a, builder) -> Value` — `builder.ins().ceil(a)`

**Math:**
- `emit_sqrt(a, builder) -> Value` — `builder.ins().sqrt(a)`

**Conversions:**
- `emit_from_sint(a, builder) -> Value` — `builder.ins().fcvt_from_sint(types::F32, a)`
- `emit_to_sint(a, builder) -> Value` — `builder.ins().fcvt_to_sint(types::I32, a)`
- `emit_from_uint(a, builder) -> Value` — `builder.ins().fcvt_from_uint(types::F32, a)`
- `emit_to_uint(a, builder) -> Value` — `builder.ins().fcvt_to_uint(types::I32, a)`

**Signatures:**
- `map_signature(sig: &Signature) -> Signature` — `sig.clone()` (identity)

### Design decisions

Use `&mut FunctionBuilder` (not `FuncCursor`). The call sites all have a
builder available, and FunctionBuilder provides the full instruction API.
The Q32Strategy will need multi-instruction sequences, which FunctionBuilder
supports.

The enum match dispatch is a single branch — effectively free. When
Q32Strategy is added later, add a variant:

```rust
pub enum NumericMode {
    Float(FloatStrategy),
    Q32(Q32Strategy),  // added in Plan B
}
```

### Register in module tree

Add `pub mod numeric;` to `frontend/codegen/mod.rs`.

### Tests

No new tests needed for FloatStrategy — it's identity. The existing test
suite validates it. A simple unit test can verify `scalar_type()` returns
`types::F32`.
