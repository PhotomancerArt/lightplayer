# Phase 4: Update Constant Emission

Every `f32const(...)` in the codegen becomes `ctx.emit_float_const(...)`.

## `expr/literal.rs`

```rust
// Before:
Expr::FloatConst(f, _) => {
    let val = ctx.builder.ins().f32const(*f);

// After:
Expr::FloatConst(f, _) => {
    let val = ctx.emit_float_const(*f);
```

1 site.

## `expr/variable.rs`

Constant value emission for floats, vectors, and matrices:

```rust
// Before:
ConstValue::Float(f) => vec![ctx.builder.ins().f32const(*f)],
ConstValue::Vec2(v) => vec![
    ctx.builder.ins().f32const(v[0]),
    ctx.builder.ins().f32const(v[1]),
],
// ... Vec3, Vec4, Mat2 similarly

// After:
ConstValue::Float(f) => vec![ctx.emit_float_const(*f)],
ConstValue::Vec2(v) => vec![
    ctx.emit_float_const(v[0]),
    ctx.emit_float_const(v[1]),
],
```

~15 sites (Float + Vec2×2 + Vec3×3 + Vec4×4 + Mat2×4).

## `expr/constructor.rs`

Matrix identity/zero padding:

```rust
// Before:
let zero = ctx.builder.ins().f32const(0.0);
// ...
ctx.builder.ins().f32const(1.0)  // diagonal
ctx.builder.ins().f32const(0.0)  // off-diagonal

// After:
let zero = ctx.emit_float_const(0.0);
ctx.emit_float_const(1.0)
ctx.emit_float_const(0.0)
```

~3 sites.

## `stmt/declaration.rs`

Array zero-initialization for float arrays:

```rust
// Before:
GlslType::Float => ctx.builder.ins().f32const(0.0),

// After:
GlslType::Float => ctx.emit_float_const(0.0),
```

2 sites.

## Builtin files

Constants used in builtins — these are covered in phases 6/7 alongside
the operations they're part of (sign's 0.0/1.0/-1.0, trigonometric
conversion constants, etc.).

## Total: ~21 sites

All mechanical. Replace `ctx.builder.ins().f32const(X)` with
`ctx.emit_float_const(X)`.
