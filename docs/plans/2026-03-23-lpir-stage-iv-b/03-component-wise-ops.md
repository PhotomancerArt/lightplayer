# Phase 3: Component-Wise Operations

## Scope

Add vector handling for `Binary`, `Unary`, `Select`, and `As` (cast)
expressions. These all follow the same pattern: apply the existing scalar
operation to each component independently. Includes scalar broadcast
detection for binary ops (e.g. `vec3 * float`).

## Implementation Details

### Scalar broadcast detection

For `Binary { left, right }`, both operands may be vectors, or one may
be scalar (broadcast). Use `expr_type_inner` to get the widths:

```rust
let left_inner = expr_type_inner(ctx.module, ctx.func, left)?;
let right_inner = expr_type_inner(ctx.module, ctx.func, right)?;
let left_width = naga_type_width(left_inner);
let right_width = naga_type_width(right_inner);
```

Cases:
- Both width 1 → scalar path (existing)
- Same width > 1 → component-wise, zip VRegs
- One width 1, other width N → broadcast: reuse the scalar VReg for
  each component of the wider operand

### `lower_expr.rs` — `Binary` (vector)

```rust
fn lower_binary_vec(
    ctx: &mut LowerCtx<'_>,
    op: BinaryOperator,
    left: Handle<Expression>,
    right: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    let left_vs = lower_expr_vec(ctx, left)?;
    let right_vs = lower_expr_vec(ctx, right)?;
    let n = left_vs.len().max(right_vs.len());

    let lk = expr_scalar_kind(ctx.module, ctx.func, left)?;
    let mut result = VRegVec::new();
    for i in 0..n {
        let l = left_vs[i.min(left_vs.len() - 1)];
        let r = right_vs[i.min(right_vs.len() - 1)];
        let v = lower_binary_scalar(ctx, op, l, r, lk)?;
        result.push(v);
    }
    Ok(result)
}
```

`lower_binary_scalar` is the existing per-component logic extracted from
the current `lower_binary` function. The `i.min(len - 1)` pattern
handles broadcast: if one side has length 1, index 0 is reused for all
components.

Note: for comparison operators (`==`, `<`, etc.) on vectors, the result
is a vector of bools (i32). This matches Naga semantics — each component
compared independently.

### `lower_expr.rs` — `Unary` (vector)

```rust
fn lower_unary_vec(
    ctx: &mut LowerCtx<'_>,
    op: UnaryOperator,
    inner: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    let inner_vs = lower_expr_vec(ctx, inner)?;
    let k = expr_scalar_kind(ctx.module, ctx.func, inner)?;
    let mut result = VRegVec::new();
    for &src in &inner_vs {
        let v = lower_unary_scalar(ctx, op, src, k)?;
        result.push(v);
    }
    Ok(result)
}
```

Extract existing unary logic into `lower_unary_scalar(ctx, op, src, kind)`.

### `lower_expr.rs` — `Select` (vector)

Naga `Select { condition, accept, reject }` where condition may be a
scalar bool or a bool vector. If condition is scalar and accept/reject
are vectors, broadcast the condition:

```rust
fn lower_select_vec(
    ctx: &mut LowerCtx<'_>,
    condition: Handle<Expression>,
    accept: Handle<Expression>,
    reject: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    let cond_vs = lower_expr_vec(ctx, condition)?;
    let accept_vs = lower_expr_vec(ctx, accept)?;
    let reject_vs = lower_expr_vec(ctx, reject)?;
    let n = accept_vs.len();
    let ty = expr_scalar_kind(ctx.module, ctx.func, accept)?;
    let dst_ty = match ty {
        ScalarKind::Float => IrType::F32,
        _ => IrType::I32,
    };
    let mut result = VRegVec::new();
    for i in 0..n {
        let c = cond_vs[i.min(cond_vs.len() - 1)];
        let dst = ctx.fb.alloc_vreg(dst_ty);
        ctx.fb.push(Op::Select {
            dst,
            cond: c,
            if_true: accept_vs[i],
            if_false: reject_vs[i],
        });
        result.push(dst);
    }
    Ok(result)
}
```

### `lower_expr.rs` — `As` (vector cast)

Component-wise cast:

```rust
fn lower_as_vec(
    ctx: &mut LowerCtx<'_>,
    inner: Handle<Expression>,
    target_kind: ScalarKind,
) -> Result<VRegVec, LowerError> {
    let inner_vs = lower_expr_vec(ctx, inner)?;
    let src_k = expr_scalar_kind(ctx.module, ctx.func, inner)?;
    if src_k == target_kind {
        return Ok(inner_vs);
    }
    let mut result = VRegVec::new();
    for &src in &inner_vs {
        let v = lower_as_scalar(ctx, src, src_k, target_kind)?;
        result.push(v);
    }
    Ok(result)
}
```

Extract existing `lower_as` per-component logic into
`lower_as_scalar(ctx, src, src_kind, target_kind)`.

### Refactoring existing scalar functions

The existing `lower_binary`, `lower_unary`, `lower_select`, `lower_as`
functions should be refactored:

1. Extract the per-component logic into `_scalar` helper functions.
2. The `_vec` functions loop over components and call the `_scalar`
   helpers.
3. The top-level dispatch in `lower_expr_vec_uncached` calls the `_vec`
   functions unconditionally — they handle both scalar and vector cases
   (scalar is just N=1).

### Matrix binary ops

`Binary` on matrices follows the same component-wise pattern for `+`,
`-`, `*` (when both operands are matrices of the same size or one is
scalar). The exception is `mat * vec` and `mat * mat` which are
matrix-specific — those are handled in phase 5 via `lower_matrix.rs`.

Detection: check if the binary op is `Multiply` and the operand types
are `Matrix × Vector`, `Vector × Matrix`, or `Matrix × Matrix`. If so,
delegate to `lower_matrix`. Otherwise, component-wise.

## Validate

```
cargo test -p lp-glsl-naga
cargo +nightly fmt -p lp-glsl-naga -- --check
cargo clippy -p lp-glsl-naga
```

Vector arithmetic filetests (binary ops, unary ops, casts on vectors)
should now lower.
