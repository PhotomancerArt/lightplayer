# Phase 4: Q32 Inline Ops and Casts

## Scope

Implement Q32-specific inline emission for float ops without builtins (fneg,
fabs, fmin, fmax, ffloor, fceil, ftrunc), Q32 comparisons (fcmp → icmp),
and Q32 cast ops (FtoiSat, Itof). Port from old crate's `Q32Strategy` in
`numeric.rs`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Add Q32 inline emitters to `q32.rs`

Port from `lps-cranelift/src/frontend/codegen/numeric.rs` Q32Strategy
methods. Each function takes `&mut FunctionBuilder` + operand `Value`s,
returns a result `Value`.

```rust
use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_frontend::FunctionBuilder;

const Q32_SHIFT: i64 = 16;
const Q32_FRAC_MASK: i64 = (1 << Q32_SHIFT) - 1; // 0xFFFF
const Q32_INT_MASK: i64 = !Q32_FRAC_MASK;         // 0xFFFF_0000 as i64

pub(crate) fn emit_fneg(builder: &mut FunctionBuilder, v: Value) -> Value {
    builder.ins().ineg(v)
}

pub(crate) fn emit_fabs(builder: &mut FunctionBuilder, v: Value) -> Value {
    let zero = builder.ins().iconst(types::I32, 0);
    let neg = builder.ins().ineg(v);
    let is_neg = builder.ins().icmp(IntCC::SignedLessThan, v, zero);
    builder.ins().select(is_neg, neg, v)
}

pub(crate) fn emit_fmin(builder: &mut FunctionBuilder, a: Value, b: Value) -> Value {
    let cmp = builder.ins().icmp(IntCC::SignedLessThanOrEqual, a, b);
    builder.ins().select(cmp, a, b)
}

pub(crate) fn emit_fmax(builder: &mut FunctionBuilder, a: Value, b: Value) -> Value {
    let cmp = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, a, b);
    builder.ins().select(cmp, a, b)
}
```

**Floor** — round toward −∞. For Q16.16: mask out fractional bits, but for
negative values with nonzero fraction, subtract ONE first. Port from old
crate's `Q32Strategy::emit_floor`:

```rust
pub(crate) fn emit_ffloor(builder: &mut FunctionBuilder, v: Value) -> Value {
    // Mask off fractional bits (toward zero)
    let int_mask = builder.ins().iconst(types::I32, Q32_INT_MASK);
    let truncated = builder.ins().band(v, int_mask);
    // For negative values with nonzero fraction, subtract 1.0 (= 1<<16)
    let frac_mask = builder.ins().iconst(types::I32, Q32_FRAC_MASK);
    let frac = builder.ins().band(v, frac_mask);
    let zero = builder.ins().iconst(types::I32, 0);
    let has_frac = builder.ins().icmp(IntCC::NotEqual, frac, zero);
    let is_neg = builder.ins().icmp(IntCC::SignedLessThan, v, zero);
    let needs_adjust = builder.ins().band(has_frac, is_neg);
    let one = builder.ins().iconst(types::I32, 1 << Q32_SHIFT);
    let adjusted = builder.ins().isub(truncated, one);
    builder.ins().select(needs_adjust, adjusted, truncated)
}
```

**Ceil** — round toward +∞. Mirror of floor for positive values:

```rust
pub(crate) fn emit_fceil(builder: &mut FunctionBuilder, v: Value) -> Value {
    let int_mask = builder.ins().iconst(types::I32, Q32_INT_MASK);
    let truncated = builder.ins().band(v, int_mask);
    let frac_mask = builder.ins().iconst(types::I32, Q32_FRAC_MASK);
    let frac = builder.ins().band(v, frac_mask);
    let zero = builder.ins().iconst(types::I32, 0);
    let has_frac = builder.ins().icmp(IntCC::NotEqual, frac, zero);
    let is_pos = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, v, zero);
    let needs_adjust = builder.ins().band(has_frac, is_pos);
    let one = builder.ins().iconst(types::I32, 1 << Q32_SHIFT);
    let adjusted = builder.ins().iadd(truncated, one);
    builder.ins().select(needs_adjust, adjusted, truncated)
}
```

**Trunc** — round toward zero (just mask off fractional bits):

```rust
pub(crate) fn emit_ftrunc(builder: &mut FunctionBuilder, v: Value) -> Value {
    let int_mask = builder.ins().iconst(types::I32, Q32_INT_MASK);
    builder.ins().band(v, int_mask)
}
```

### 2. Q32 cast ops in `q32.rs`

Port from old crate's `Q32Strategy::emit_to_sint`, `emit_from_sint`,
`emit_to_uint`, `emit_from_uint`.

```rust
/// Q16.16 → signed integer (truncate toward zero, like C cast)
pub(crate) fn emit_to_sint(builder: &mut FunctionBuilder, v: Value) -> Value {
    let shift = builder.ins().iconst(types::I32, Q32_SHIFT);
    let zero = builder.ins().iconst(types::I32, 0);
    let bias_mask = builder.ins().iconst(types::I32, Q32_FRAC_MASK);
    let is_neg = builder.ins().icmp(IntCC::SignedLessThan, v, zero);
    let biased = builder.ins().iadd(v, bias_mask);
    let biased_value = builder.ins().select(is_neg, biased, v);
    builder.ins().sshr(biased_value, shift)
}

/// Signed integer → Q16.16 (clamp to representable range, then shift)
pub(crate) fn emit_from_sint(builder: &mut FunctionBuilder, v: Value) -> Value {
    let shift = builder.ins().iconst(types::I32, Q32_SHIFT);
    let max_int = builder.ins().iconst(types::I32, 32767);
    let min_int = builder.ins().iconst(types::I32, -32768);
    let clamped = builder.ins().smin(v, max_int);
    let clamped = builder.ins().smax(clamped, min_int);
    builder.ins().ishl(clamped, shift)
}

/// Q16.16 → unsigned integer (clamp negatives to 0)
pub(crate) fn emit_to_uint(builder: &mut FunctionBuilder, v: Value) -> Value {
    let trunc = emit_to_sint(builder, v);
    let zero = builder.ins().iconst(types::I32, 0);
    let is_neg = builder.ins().icmp(IntCC::SignedLessThan, trunc, zero);
    builder.ins().select(is_neg, zero, trunc)
}

/// Unsigned integer → Q16.16 (shift left, no sign extension needed)
pub(crate) fn emit_from_uint(builder: &mut FunctionBuilder, v: Value) -> Value {
    let shift = builder.ins().iconst(types::I32, Q32_SHIFT);
    builder.ins().ishl(v, shift)
}
```

### 3. Update `emit/scalar.rs` — Q32 dispatch

For each float op that has an inline Q32 equivalent, add the mode dispatch:

```rust
Op::Fneg { dst, src } => {
    let a = use_v(builder, vars, *src);
    match ctx.float_mode {
        FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| bd.ins().fneg(a)),
        FloatMode::Q32 => {
            let out = crate::q32::emit_fneg(builder, a);
            def_v(builder, vars, *dst, out);
        }
    }
}
```

Same pattern for `Fabs`, `Fmin`, `Fmax`, `Ffloor`, `Fceil`, `Ftrunc`.

Float comparisons in Q32 — use `icmp` instead of `fcmp`:

```rust
Op::Feq { dst, lhs, rhs } => {
    let a = use_v(builder, vars, *lhs);
    let b = use_v(builder, vars, *rhs);
    match ctx.float_mode {
        FloatMode::F32 => {
            let cmp = builder.ins().fcmp(FloatCC::Equal, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        FloatMode::Q32 => {
            let cmp = builder.ins().icmp(IntCC::Equal, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
    }
}
```

For Q32 float comparisons, the mapping is:

- `Feq` → `IntCC::Equal`
- `Fne` → `IntCC::NotEqual`
- `Flt` → `IntCC::SignedLessThan`
- `Fle` → `IntCC::SignedLessThanOrEqual`
- `Fgt` → `IntCC::SignedGreaterThan`
- `Fge` → `IntCC::SignedGreaterThanOrEqual`

Cast ops:

```rust
Op::FtoiSatS { dst, src } => {
    let a = use_v(builder, vars, *src);
    match ctx.float_mode {
        FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| bd.ins().fcvt_to_sint_sat(types::I32, a)),
        FloatMode::Q32 => {
            let out = crate::q32::emit_to_sint(builder, a);
            def_v(builder, vars, *dst, out);
        }
    }
}
```

Same for `FtoiSatU`, `ItofS`, `ItofU`.

### 4. Tests

**`test_q32_fneg`**:

```
func @neg(v0:f32) -> f32 {
  v1:f32 = fneg v0
  return v1
}
```

Call with Q32-encoded 1.0 (65536), verify result is -65536.

**`test_q32_fabs`**:

```
func @abs(v0:f32) -> f32 {
  v1:f32 = fabs v0
  return v1
}
```

Call with Q32 -1.0 (-65536), verify result is 65536.

**`test_q32_fmin_fmax`**:

```
func @minmax(v0:f32, v1:f32) -> f32, f32 {
  v2:f32 = fmin v0, v1
  v3:f32 = fmax v0, v1
  return v2, v3
}
```

Call with Q32 3.0 and 1.0, verify min=1.0 and max=3.0.

**`test_q32_floor_ceil_trunc`**:

```
func @floor_it(v0:f32) -> f32 {
  v1:f32 = ffloor v0
  return v1
}
```

Call with Q32 1.75 (`1.75 * 65536 = 114688`), verify result is Q32 1.0 (65536).
Call with Q32 -1.75, verify result is Q32 -2.0 (-131072).

**`test_q32_comparison`**:

```
func @is_positive(v0:f32) -> i32 {
  v1:f32 = fconst 0.0
  v2:i32 = fgt v0, v1
  return v2
}
```

Call with Q32 1.0, verify returns 1. Call with Q32 -1.0, verify returns 0.

**`test_q32_ftoi_itof`**:

```
func @roundtrip(v0:f32) -> f32 {
  v1:i32 = ftoi_sat_s v0
  v2:f32 = itof_s v1
  return v2
}
```

Call with Q32 2.75, verify returns Q32 2.0 (fractional truncated).

## Validate

```
cargo check -p lpvm-cranelift
cargo test -p lpvm-cranelift
```
