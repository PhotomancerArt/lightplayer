# Phase 3: Update Scalar Arithmetic Call Sites

The core arithmetic operations. These are the most numerous and most
important call sites.

## `expr/binary.rs` — `emit_scalar_binary_op`

This is the central dispatch for scalar binary operations. The Float
branches all hardcode CLIF float instructions:

```rust
// Before:
BinaryOp::Add => match operand_ty {
    GlslType::Float => ctx.builder.ins().fadd(lhs, rhs),
    GlslType::Int | GlslType::UInt => ctx.builder.ins().iadd(lhs, rhs),
    ...
}

// After:
BinaryOp::Add => match operand_ty {
    GlslType::Float => ctx.emit_float_add(lhs, rhs),
    GlslType::Int | GlslType::UInt => ctx.builder.ins().iadd(lhs, rhs),
    ...
}
```

Changes (6 sites in emit_scalar_binary_op):
- Line ~177: `fadd(lhs, rhs)` → `ctx.emit_float_add(lhs, rhs)`
- Line ~187: `fsub(lhs, rhs)` → `ctx.emit_float_sub(lhs, rhs)`
- Line ~197: `fmul(lhs, rhs)` → `ctx.emit_float_mul(lhs, rhs)`
- Line ~212: `fdiv(lhs, rhs)` → `ctx.emit_float_div(lhs, rhs)`

Note: Int/UInt branches remain unchanged — they use `iadd`, `isub`, etc.
directly. The strategy only applies to float operations.

## `expr/unary.rs`

```rust
// Before:
GlslType::Float => ctx.builder.ins().fneg(val),

// After:
GlslType::Float => ctx.emit_float_neg(val),
```

1 site.

## `expr/incdec.rs`

```rust
// Before:
let one = ctx.builder.ins().f32const(1.0);
// ...
ctx.builder.ins().fadd(*old_value, one)
// or:
ctx.builder.ins().fsub(*old_value, one)

// After:
let one = ctx.emit_float_const(1.0);
// ...
ctx.emit_float_add(*old_value, one)
// or:
ctx.emit_float_sub(*old_value, one)
```

3 sites (const + add + sub).

## Total: 10 call sites

All are mechanical replacements. Each site is guarded by a
`GlslType::Float =>` match arm, so it's clear these are float operations.

## Validate

```bash
cargo test --features std -p lps-compiler
```

All tests must pass unchanged. The FloatStrategy emits identical instructions.
