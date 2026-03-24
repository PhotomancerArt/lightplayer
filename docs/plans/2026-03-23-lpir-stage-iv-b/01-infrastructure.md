# Phase 1: Multi-VReg Infrastructure

## Scope

Update `LowerCtx`, type helpers, and the expression cache to support
multi-VReg results. After this phase, the plumbing exists for vector
expressions even though no vector expression variants are lowered yet.
Existing scalar tests must still pass.

## Code Organization Reminders

- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.

## Implementation Details

### `Cargo.toml` — add `smallvec`

```toml
[dependencies]
smallvec = { version = "1", default-features = false }
```

### `lower_ctx.rs` — cache and map types

Change the expression cache:

```rust
use smallvec::SmallVec;

pub(crate) type VRegVec = SmallVec<[VReg; 4]>;

pub(crate) struct LowerCtx<'a> {
    // ...
    pub expr_cache: Vec<Option<VRegVec>>,
    pub local_map: BTreeMap<Handle<LocalVariable>, VRegVec>,
    pub param_aliases: BTreeMap<Handle<LocalVariable>, VRegVec>,
    // ...
}
```

### `lower_ctx.rs` — `naga_type_to_ir_types`

New function returning multiple IR types for a Naga type:

```rust
pub(crate) fn naga_type_to_ir_types(inner: &TypeInner) -> Result<SmallVec<[IrType; 4]>, LowerError> {
    match inner {
        TypeInner::Scalar(scalar) => {
            Ok(smallvec![naga_scalar_to_ir_type(scalar.kind)?])
        }
        TypeInner::Vector { size, scalar, .. } => {
            let t = naga_scalar_to_ir_type(scalar.kind)?;
            let n = *size as usize;
            Ok(SmallVec::from_elem(t, n))
        }
        TypeInner::Matrix { columns, rows, scalar, .. } => {
            let t = naga_scalar_to_ir_type(scalar.kind)?;
            let n = (*columns as usize) * (*rows as usize);
            Ok(SmallVec::from_elem(t, n))
        }
        _ => Err(LowerError::UnsupportedType(format!(
            "unsupported type for LPIR: {inner:?}"
        ))),
    }
}
```

Keep the old `naga_type_to_ir_type` that asserts scalar, or refactor it
to call `naga_type_to_ir_types` and assert length 1 — caller's choice.

### `lower_ctx.rs` — `naga_type_width`

```rust
pub(crate) fn naga_type_width(inner: &TypeInner) -> usize {
    match inner {
        TypeInner::Scalar(_) => 1,
        TypeInner::Vector { size, .. } => *size as usize,
        TypeInner::Matrix { columns, rows, .. } => (*columns as usize) * (*rows as usize),
        _ => 1,
    }
}
```

### `lower_ctx.rs` — parameter and local setup

Update `LowerCtx::new()`:

**Parameters**: For each function argument, call `naga_type_to_ir_types`.
Add one `fb.add_param(ty)` per IR type. Track the mapping from argument
index to the starting VReg and width.

```rust
let mut param_offset = 0u32;
let mut arg_vreg_map: BTreeMap<u32, VRegVec> = BTreeMap::new();
for (i, arg) in func.arguments.iter().enumerate() {
    let inner = &module.types[arg.ty].inner;
    let tys = naga_type_to_ir_types(inner)?;
    let mut vregs = VRegVec::new();
    for ty in &tys {
        fb.add_param(*ty);
        vregs.push(VReg(param_offset));
        param_offset += 1;
    }
    arg_vreg_map.insert(i as u32, vregs);
}
```

**Param aliases**: For `Store(LocalVariable, FunctionArgument(i))`, the
local's VReg vec aliases the argument's VReg vec.

```rust
for (lv, arg_i) in &param_idx {
    if let Some(vs) = arg_vreg_map.get(arg_i) {
        param_aliases.insert(*lv, vs.clone());
    }
}
```

**Locals**: For each local variable, allocate N VRegs:

```rust
for (lv_handle, var) in func.local_variables.iter() {
    if param_aliases.contains_key(&lv_handle) {
        continue;
    }
    let inner = &module.types[var.ty].inner;
    let tys = naga_type_to_ir_types(inner)?;
    let mut vregs = VRegVec::new();
    for ty in &tys {
        vregs.push(fb.alloc_vreg(*ty));
    }
    local_map.insert(lv_handle, vregs);
}
```

**Local init**: For locals with an initializer, lower the init expression
to a VRegVec and emit N `Copy` ops:

```rust
for (lv_handle, var) in func.local_variables.iter() {
    if ctx.param_aliases.contains_key(&lv_handle) { continue; }
    let Some(init_h) = var.init else { continue; };
    let dsts = ctx.local_map.get(&lv_handle).unwrap().clone();
    let srcs = lower_expr::lower_expr_vec(&mut ctx, init_h)?;
    for (d, s) in dsts.iter().zip(srcs.iter()) {
        ctx.fb.push(Op::Copy { dst: *d, src: *s });
    }
}
```

### `lower_ctx.rs` — `ensure_expr_vec` and `ensure_expr`

```rust
pub(crate) fn ensure_expr_vec(
    &mut self,
    expr: Handle<naga::Expression>,
) -> Result<VRegVec, LowerError> {
    let i = expr.index();
    if let Some(vs) = self.expr_cache.get(i).and_then(|c| c.as_ref()) {
        return Ok(vs.clone());
    }
    let vs = lower_expr::lower_expr_vec(self, expr)?;
    if let Some(slot) = self.expr_cache.get_mut(i) {
        *slot = Some(vs.clone());
    }
    Ok(vs)
}

pub(crate) fn ensure_expr(
    &mut self,
    expr: Handle<naga::Expression>,
) -> Result<VReg, LowerError> {
    let vs = self.ensure_expr_vec(expr)?;
    if vs.len() != 1 {
        return Err(LowerError::Internal(format!(
            "expected scalar expression, got {} components", vs.len()
        )));
    }
    Ok(vs[0])
}
```

### `lower_ctx.rs` — `resolve_local`

Returns `VRegVec`:

```rust
pub(crate) fn resolve_local(
    &self,
    lv: Handle<LocalVariable>,
) -> Result<VRegVec, LowerError> {
    if let Some(v) = self.param_aliases.get(&lv) {
        return Ok(v.clone());
    }
    self.local_map
        .get(&lv)
        .cloned()
        .ok_or_else(|| LowerError::Internal(format!("unknown local variable {lv:?}")))
}
```

### `lower.rs` — `func_return_ir_types`

Handle vector and matrix returns:

```rust
fn func_return_ir_types(module: &Module, func: &Function) -> Result<Vec<IrType>, LowerError> {
    let Some(res) = &func.result else {
        return Ok(Vec::new());
    };
    let inner = &module.types[res.ty].inner;
    let tys = naga_type_to_ir_types(inner)?;
    Ok(tys.to_vec())
}
```

### `expr_scalar.rs` — handle vector/matrix types

Update `type_handle_scalar_kind` to extract element kind from vectors
and matrices:

```rust
pub(crate) fn type_handle_scalar_kind(
    module: &Module,
    ty: Handle<naga::Type>,
) -> Result<ScalarKind, LowerError> {
    match &module.types[ty].inner {
        TypeInner::Scalar(s) => Ok(s.kind),
        TypeInner::Vector { scalar, .. } => Ok(scalar.kind),
        TypeInner::Matrix { scalar, .. } => Ok(scalar.kind),
        _ => Err(LowerError::UnsupportedType(String::from(
            "expected scalar/vector/matrix type",
        ))),
    }
}
```

Update `expr_scalar_kind` to handle `Compose`, `Splat`, `Swizzle`,
`AccessIndex`:

```rust
Expression::Compose { ty, .. } => type_handle_scalar_kind(module, *ty),
Expression::Splat { value, .. } => expr_scalar_kind(module, func, *value),
Expression::Swizzle { vector, .. } => expr_scalar_kind(module, func, *vector),
Expression::AccessIndex { base, .. } => expr_scalar_kind(module, func, *base),
Expression::Access { base, .. } => expr_scalar_kind(module, func, *base),
```

Update `ZeroValue` to handle vector/matrix types:

```rust
Expression::ZeroValue(ty_h) => type_handle_scalar_kind(module, *ty_h),
```

### `lower_expr.rs` — initial adapter

Change `lower_expr` and `lower_expr_uncached` to call through
`lower_expr_vec` internally and assert scalar. This is a transitional
step — existing scalar arms stay in `lower_expr_uncached` but the new
multi-VReg entry point is `lower_expr_vec`. In this phase,
`lower_expr_vec` for non-scalar types can error with "not yet
implemented" — the actual vector arms come in later phases.

```rust
pub(crate) fn lower_expr_vec(
    ctx: &mut LowerCtx<'_>,
    expr: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    let i = expr.index();
    if let Some(vs) = ctx.expr_cache.get(i).and_then(|c| c.as_ref()) {
        return Ok(vs.clone());
    }
    let vs = lower_expr_vec_uncached(ctx, expr)?;
    if let Some(slot) = ctx.expr_cache.get_mut(i) {
        *slot = Some(vs.clone());
    }
    Ok(vs)
}
```

For this phase, `lower_expr_vec_uncached` delegates to the existing
scalar `lower_expr_uncached` and wraps the result:

```rust
fn lower_expr_vec_uncached(
    ctx: &mut LowerCtx<'_>,
    expr: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    // TODO: vector/matrix arms added in phases 2-5
    let v = lower_expr_uncached(ctx, expr)?;
    Ok(smallvec![v])
}
```

### `lower_stmt.rs` — adapt to `VRegVec`

Update `Store` to use `resolve_local` returning `VRegVec`. For now,
assert length 1 (vector stores come in phase 6):

```rust
Statement::Store { pointer, value } => {
    let lv = store_pointer_local(ctx.func, *pointer)?;
    let dsts = ctx.resolve_local(lv)?;
    let srcs = ctx.ensure_expr_vec(*value)?;
    for (d, s) in dsts.iter().zip(srcs.iter()) {
        ctx.fb.push(Op::Copy { dst: *d, src: *s });
    }
    Ok(())
}
```

Update `Return` to push all VRegs:

```rust
Statement::Return { value } => match value {
    Some(expr) => {
        let vs = ctx.ensure_expr_vec(*expr)?;
        ctx.fb.push_return(&vs);
        Ok(())
    }
    None => {
        ctx.fb.push_return(&[]);
        Ok(())
    }
},
```

Update `lower_user_call` to flatten vector args and allocate multi-VReg
results. See phase 6 for full details, but the infrastructure change
(using `ensure_expr_vec` and `naga_type_to_ir_types`) belongs here.

## Validate

```
cargo test -p lp-glsl-naga
cargo +nightly fmt -p lp-glsl-naga -- --check
cargo clippy -p lp-glsl-naga
```

All existing scalar tests must still pass. The expression cache type
change is internal — no public API change.
