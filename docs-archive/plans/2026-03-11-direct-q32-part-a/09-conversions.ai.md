# Phase 9: Update Conversion Call Sites

## `expr/coercion.rs`

Int-to-float and float-to-int conversions. These are the CLIF `fcvt_*`
instructions.

### int → float

```rust
// Before:
Ok(ctx.builder.ins().fcvt_from_sint(types::F32, val))

// After:
Ok(ctx.emit_float_from_sint(val))
```

2 sites (int→float and bool→float paths).

### float → int

```rust
// Before:
Ok(ctx.builder.ins().fcvt_to_sint(types::I32, val))

// After:
Ok(ctx.emit_float_to_sint(val))
```

1 site.

### float → uint

```rust
// Before:
Ok(ctx.builder.ins().fcvt_to_uint(types::I32, val))

// After:
Ok(ctx.emit_float_to_uint(val))
```

1 site.

### uint → float

```rust
// Before:
Ok(ctx.builder.ins().fcvt_from_uint(types::F32, val))

// After:
Ok(ctx.emit_float_from_uint(val))
```

1 site.

## Total: 5 sites

These are important for Q32 — the conversion semantics differ significantly.
For float, `fcvt_from_sint` converts an integer to f32. For Q32,
`emit_float_from_sint` would shift the integer left by 16 bits (multiply
by 65536) to produce a Q16.16 value. The FloatStrategy just delegates to
the standard CLIF conversion instructions.

## Note on trait methods

The trait needs four conversion methods (not two as in the design doc):
- `emit_from_sint(a, builder) -> Value`
- `emit_to_sint(a, builder) -> Value`
- `emit_from_uint(a, builder) -> Value`
- `emit_to_uint(a, builder) -> Value`

Update the trait definition in phase 1 accordingly.
