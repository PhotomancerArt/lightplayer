# Phase 6: Statements, Calls, and LPFX Vector Support

## Scope

Update statement lowering for vector Store/Return/Call. Update LPFX
lowering for vector value arguments and vector out-parameters. After
this phase, full vector/matrix programs including function calls and
LPFX builtins lower correctly.

## Implementation Details

### `lower_stmt.rs` — Store (already done in phase 1)

The Store change was included in phase 1's infrastructure update. N
`Copy` ops are emitted, one per component. Verify it works for vector
locals.

### `lower_stmt.rs` — Return (already done in phase 1)

The Return change was included in phase 1. All component VRegs are
pushed. Verify for vector returns.

### `lower_stmt.rs` — user Call (vector args and results)

Update `lower_user_call`:

**Arguments**: flatten vector arguments to scalar VRegs:

```rust
let mut arg_vs = Vec::new();
for a in arguments {
    let vs = ctx.ensure_expr_vec(*a)?;
    arg_vs.extend_from_slice(&vs);
}
```

**Results**: allocate N VRegs for vector return types:

```rust
let mut result_vs = Vec::new();
if let Some(res_h) = result {
    let res_ty = f.result.as_ref().ok_or_else(|| ...)?;
    let inner = &ctx.module.types[res_ty.ty].inner;
    let ir_tys = naga_type_to_ir_types(inner)?;
    let mut vregs = VRegVec::new();
    for ty in &ir_tys {
        let v = ctx.fb.alloc_vreg(*ty);
        vregs.push(v);
        result_vs.push(v);
    }
    if let Some(slot) = ctx.expr_cache.get_mut(res_h.index()) {
        *slot = Some(vregs);
    }
}
ctx.fb.push_call(callee_ref, &arg_vs, &result_vs);
```

### `lower_lpfx.rs` — vector value arguments

Update `lpfx_arg_kinds` to handle vector value arguments:

```rust
TypeInner::Vector { size, scalar, .. } => {
    let ir_ty = naga_scalar_to_ir_type(scalar.kind)?;
    for _ in 0..(*size as usize) {
        out.push(LpfxArgKind::Value);
    }
}
```

In `build_lpfx_import_decl`, vector value params add N scalar param
types:

```rust
TypeInner::Vector { size, scalar, .. } => {
    let ir_ty = naga_scalar_to_ir_type(scalar.kind)?;
    for _ in 0..(*size as usize) {
        param_types.push(ir_ty);
    }
}
```

In `lower_lpfx_call`, flatten vector value arguments:

```rust
LpfxArgKind::Value => {
    let vs = ctx.ensure_expr_vec(arg_expr)?;
    for v in &vs {
        arg_vs.push(*v);
    }
}
```

### `lower_lpfx.rs` — vector out-parameters

New `LpfxArgKind` variant:

```rust
pub(crate) enum LpfxArgKind {
    Value,
    OutScalar(IrType),
    OutVector(IrType, u8),  // element type, component count
}
```

In `lpfx_arg_kinds`:

```rust
TypeInner::Pointer { base, space: AddressSpace::Function } => {
    match &module.types[*base].inner {
        TypeInner::Scalar(scalar) => {
            out.push(LpfxArgKind::OutScalar(naga_scalar_to_ir_type(scalar.kind)?));
        }
        TypeInner::Vector { size, scalar, .. } => {
            out.push(LpfxArgKind::OutVector(
                naga_scalar_to_ir_type(scalar.kind)?,
                *size as u8,
            ));
        }
        _ => return Err(...)
    }
}
```

In `build_lpfx_import_decl`, vector out-params contribute one `I32`
param (the slot address), same as scalar out-params:

```rust
TypeInner::Vector { .. } => {
    param_types.push(IrType::I32);
}
```

In `lower_lpfx_call`, handle `OutVector`:

```rust
LpfxArgKind::OutVector(ir_ty, count) => {
    let lv = out_pointer_local(ctx.func, arg_expr)?;
    let dsts = ctx.resolve_local(lv)?;
    let n = *count as usize;
    let slot = ctx.fb.alloc_slot(n as u32 * 4);
    let addr = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::SlotAddr { dst: addr, slot });
    arg_vs.push(addr);
    vec_outs.push((addr, dsts, *ir_ty, n));
}
```

After the call, load N components:

```rust
for (addr, dsts, ir_ty, n) in vec_outs {
    for i in 0..n {
        let tmp = ctx.fb.alloc_vreg(ir_ty);
        ctx.fb.push(Op::Load {
            dst: tmp,
            base: addr,
            offset: (i as i32) * 4,
        });
        ctx.fb.push(Op::Copy { dst: dsts[i], src: tmp });
    }
}
```

### `lower_lpfx.rs` — vector return values

If an LPFX function returns a vector (unlikely but possible), update
`build_lpfx_import_decl` and the result handling in `lower_lpfx_call`
to allocate N return VRegs. Follow the same pattern as user calls.

## Validate

```
cargo test -p lps-naga
cargo +nightly fmt -p lps-naga -- --check
cargo clippy -p lps-naga
```

LPFX filetests with vector arguments and out-parameters (especially
`lpfx_psrdnoise` with `out vec2 gradient`) should now lower.
