# Phase 5: Update Comparison Call Sites

## `expr/binary.rs` — `emit_scalar_binary_op`

All float comparisons use `fcmp` with a `FloatCC` condition code:

```rust
// Before:
BinaryOp::Equal => match operand_ty {
    GlslType::Float => ctx.builder.ins().fcmp(FloatCC::Equal, lhs, rhs),
    ...
}

// After:
BinaryOp::Equal => match operand_ty {
    GlslType::Float => ctx.emit_float_cmp(FloatCC::Equal, lhs, rhs),
    ...
}
```

6 sites:
- Equal (FloatCC::Equal)
- NonEqual (FloatCC::NotEqual)
- LessThan (FloatCC::LessThan)
- GreaterThan (FloatCC::GreaterThan)
- LessThanOrEqual (FloatCC::LessThanOrEqual)
- GreaterThanOrEqual (FloatCC::GreaterThanOrEqual)

## `expr/vector.rs`

Vector equality comparison, per-component:

```rust
// Before:
ctx.builder.ins().fcmp(..., FloatCC::Equal, lhs_vals[i], rhs_vals[i])

// After:
ctx.emit_float_cmp(FloatCC::Equal, lhs_vals[i], rhs_vals[i])
```

1 site.

## `expr/matrix.rs`

Matrix equality comparison, per-element:

```rust
// Before:
let cmp = ctx.builder.ins().fcmp(..., FloatCC::Equal, ...)

// After:
let cmp = ctx.emit_float_cmp(FloatCC::Equal, ...)
```

1 site.

## `builtins/relational.rs`

Component-wise comparison builtins (lessThan, lessThanEqual, greaterThan,
greaterThanEqual, equal, notEqual on vectors):

Each has a float-type branch that calls `fcmp`:

```rust
// Before:
GlslType::Float => self.builder.ins().fcmp(FloatCC::LessThan, x_vals[i], y_vals[i])

// After:
GlslType::Float => self.emit_float_cmp(FloatCC::LessThan, x_vals[i], y_vals[i])
```

6 sites (one per relational function).

## `expr/coercion.rs`

Float-to-bool conversion uses fcmp:

```rust
// Before:
let zero = ctx.builder.ins().f32const(0.0);
let cmp = ctx.builder.ins().fcmp(FloatCC::NotEqual, val, zero);

// After:
let zero = ctx.emit_float_const(0.0);
let cmp = ctx.emit_float_cmp(FloatCC::NotEqual, val, zero);
```

2 sites (const + cmp).

## Total: 16 sites

Note: The `emit_float_cmp` method accepts `FloatCC` even though Q32
uses integer comparisons. The Q32Strategy will translate FloatCC to
IntCC internally (e.g. FloatCC::LessThan → IntCC::SignedLessThan).
The call sites don't need to know this.
