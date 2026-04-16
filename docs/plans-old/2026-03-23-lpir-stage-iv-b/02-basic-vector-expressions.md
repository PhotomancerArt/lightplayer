# Phase 2: Basic Vector Expressions

## Scope

Add arms to `lower_expr_vec_uncached` for vector construction and access
expressions: `Compose`, `Splat`, `Swizzle`, `AccessIndex`, `ZeroValue`
(vector/matrix), `Constant` (vector), `FunctionArgument` (vector),
`Load` (vector local). After this phase, simple vector programs that
construct and access vectors lower to LPIR.

## Implementation Details

### `lower_expr.rs` — vector dispatch

In `lower_expr_vec_uncached`, check the expression type before
dispatching. If the result type is a vector or matrix, use the new
vector arms. If scalar, delegate to existing scalar path.

Use `expr_type_inner(ctx.module, ctx.func, expr)` (new helper in
`expr_scalar.rs`) to determine the result type.

### `expr_scalar.rs` — `expr_type_inner`

New function to get the full `TypeInner` for an expression:

```rust
pub(crate) fn expr_type_inner<'a>(
    module: &'a Module,
    func: &'a Function,
    expr: Handle<Expression>,
) -> Result<&'a TypeInner, LowerError>
```

Follows the same pattern as `expr_scalar_kind` but returns the full type.
For `Binary` comparison operators, returns `Scalar(Bool)`. For `As`,
constructs the target type. For `Compose`/`ZeroValue`, looks up the type
handle. For `FunctionArgument`, looks up the argument type.

### `lower_expr.rs` — `Compose`

```rust
Expression::Compose { ty, components } => {
    let mut result = VRegVec::new();
    for &comp in components {
        let vs = lower_expr_vec(ctx, comp)?;
        result.extend_from_slice(&vs);
    }
    Ok(result)
}
```

`Compose { ty: vec3, components: [x, y, z] }` where each component is
scalar → 3 VRegs. `Compose { ty: vec4, components: [vec2, float, float] }`
→ 2 + 1 + 1 = 4 VRegs. Works naturally.

### `lower_expr.rs` — `Splat`

```rust
Expression::Splat { size, value } => {
    let vs = lower_expr_vec(ctx, *value)?;
    assert!(vs.len() == 1);
    let scalar = vs[0];
    let n = *size as usize;
    Ok(SmallVec::from_elem(scalar, n))
}
```

No new ops emitted — the same VReg is reused for all components. If the
WASM emitter or register allocator later needs distinct VRegs, insert
`Copy` ops here.

### `lower_expr.rs` — `Swizzle`

```rust
Expression::Swizzle { size, vector, pattern } => {
    let base = lower_expr_vec(ctx, *vector)?;
    let n = *size as usize;
    let mut result = VRegVec::new();
    for i in 0..n {
        let comp_idx = pattern[i] as usize;
        result.push(base[comp_idx]);
    }
    Ok(result)
}
```

### `lower_expr.rs` — `AccessIndex` on vector

When base is a vector, return the single component:

```rust
Expression::AccessIndex { base, index } => {
    let base_inner = expr_type_inner(ctx.module, ctx.func, *base)?;
    match base_inner {
        TypeInner::Vector { .. } => {
            let base_vs = lower_expr_vec(ctx, *base)?;
            Ok(smallvec![base_vs[*index as usize]])
        }
        TypeInner::Matrix { rows, .. } => {
            // Column access: index selects a column (vector of `rows` components)
            let base_vs = lower_expr_vec(ctx, *base)?;
            let n = *rows as usize;
            let start = (*index as usize) * n;
            Ok(base_vs[start..start + n].into())
        }
        _ => {
            // Scalar or struct — error
            Err(LowerError::UnsupportedExpression(format!(
                "AccessIndex on {base_inner:?}"
            )))
        }
    }
}
```

### `lower_expr.rs` — `Access` (dynamic index)

Error for now per design decision Q4:

```rust
Expression::Access { .. } => {
    Err(LowerError::UnsupportedExpression(String::from(
        "dynamic vector access not supported"
    )))
}
```

### `lower_expr.rs` — `ZeroValue` (vector/matrix)

```rust
Expression::ZeroValue(ty_h) => {
    let inner = &ctx.module.types[*ty_h].inner;
    match inner {
        TypeInner::Scalar(scalar) => {
            // existing scalar path
        }
        TypeInner::Vector { size, scalar, .. } => {
            let ir_ty = naga_scalar_to_ir_type(scalar.kind)?;
            let n = *size as usize;
            let mut result = VRegVec::new();
            for _ in 0..n {
                let d = ctx.fb.alloc_vreg(ir_ty);
                push_zero(ctx, d, scalar.kind);
                result.push(d);
            }
            Ok(result)
        }
        TypeInner::Matrix { columns, rows, scalar, .. } => {
            let ir_ty = naga_scalar_to_ir_type(scalar.kind)?;
            let n = (*columns as usize) * (*rows as usize);
            let mut result = VRegVec::new();
            for _ in 0..n {
                let d = ctx.fb.alloc_vreg(ir_ty);
                push_zero(ctx, d, scalar.kind);
                result.push(d);
            }
            Ok(result)
        }
        _ => Err(...)
    }
}
```

Extract a `push_zero(ctx, dst, kind)` helper that emits `FconstF32 0.0`
or `IconstI32 0`.

### `lower_expr.rs` — `Constant` (vector)

Naga global constants for vectors use `Compose` in the global expression
arena. Recurse into global expressions:

```rust
Expression::Constant(h) => {
    let init = ctx.module.constants[*h].init;
    lower_global_expr_vec(ctx, init)
}
```

Where `lower_global_expr_vec` handles `Compose` of literals in the
global arena.

### `lower_expr.rs` — `FunctionArgument` (vector)

The VRegs for a vector argument were set up in `LowerCtx::new` (phase 1).
The expression `FunctionArgument(i)` maps to consecutive VRegs starting
at the precomputed offset:

```rust
Expression::FunctionArgument(i) => {
    // arg_vreg_map populated in LowerCtx::new
    ctx.arg_vregs(*i)
}
```

Add `arg_vregs(idx: u32) → VRegVec` method to `LowerCtx` that returns
the stored VReg vector for parameter index `idx`.

### `lower_expr.rs` — `Load` (vector local)

```rust
Expression::Load { pointer } => match &ctx.func.expressions[*pointer] {
    Expression::LocalVariable(lv) => ctx.resolve_local(*lv),
    _ => Err(LowerError::UnsupportedExpression(String::from(
        "Load from non-local pointer",
    ))),
},
```

`resolve_local` already returns `VRegVec` from phase 1.

### `lower_expr.rs` — `CallResult` (vector)

Already pre-populated in expr cache by statement lowering. Return the
cached VRegVec:

```rust
Expression::CallResult(_) => {
    let i = expr.index();
    ctx.expr_cache.get(i).and_then(|c| c.as_ref()).cloned()
        .ok_or_else(|| LowerError::Internal(String::from(
            "CallResult used before matching Call statement",
        )))
}
```

## Validate

```
cargo test -p lps-frontend
cargo +nightly fmt -p lps-frontend -- --check
cargo clippy -p lps-frontend
```

Existing scalar tests pass. Simple vector construction filetests
(Compose, Splat, Swizzle, AccessIndex) should now lower without error.
