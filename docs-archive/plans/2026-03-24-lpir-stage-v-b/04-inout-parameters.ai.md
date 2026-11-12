# Phase 4: Inout/Out Parameter Support

## Scope

Handle `inout` and `out` function parameters using slot-based copy-in/copy-out,
matching the Cranelift backend's ABI convention.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Background

Naga represents `inout`/`out` parameters as `Pointer { base, space: Function }`
types. Inside the callee, `Expression::Load { pointer: FunctionArgument(i) }`
reads the value and `Statement::Store { pointer: FunctionArgument(i), value }`
writes it.

The Cranelift backend passes a pointer to a temporary stack slot. We do the
same using LPIR slots.

## Implementation

### `lower_ctx.rs` — Callee signature

In `LowerCtx::new`, when iterating `func.arguments`:

```rust
for (i, arg) in func.arguments.iter().enumerate() {
    let inner = &module.types[arg.ty].inner;
    match inner {
        TypeInner::Pointer { base, .. } => {
            // inout/out param: single i32 address parameter
            let addr = fb.add_param(IrType::I32);
            arg_vregs.insert(i as u32, smallvec![addr]);
            // Mark this argument as pointer-based for Load/Store handling
            pointer_args.insert(i as u32, *base);
        }
        _ => {
            // existing: expand to scalar vregs
            let tys = naga_type_to_ir_types(inner)?;
            // ...existing code...
        }
    }
}
```

Add `pointer_args: BTreeMap<u32, Handle<naga::Type>>` field to `LowerCtx` to
track which arguments are pointer-typed and their base types.

### `lower_expr.rs` — Load from pointer argument

When `Expression::Load { pointer }` encounters `FunctionArgument(i)` and `i`
is in `pointer_args`:

```rust
Expression::Load { pointer } => {
    match &ctx.func.expressions[*pointer] {
        Expression::FunctionArgument(i) if ctx.pointer_args.contains_key(i) => {
            let base_ty_h = ctx.pointer_args[i];
            let base_inner = &ctx.module.types[base_ty_h].inner;
            let ir_tys = naga_type_to_ir_types(base_inner)?;
            let addr = ctx.arg_vregs_for(*i)?[0];
            let mut vregs = VRegVec::new();
            for (offset_idx, ty) in ir_tys.iter().enumerate() {
                let dst = ctx.fb.alloc_vreg(*ty);
                let off_vreg = /* compute addr + offset_idx * 4 */;
                ctx.fb.push(Op::Load { dst, addr: off_vreg });
                vregs.push(dst);
            }
            Ok(vregs)
        }
        // ...existing cases...
    }
}
```

### `lower_stmt.rs` — Store to pointer argument

When `Statement::Store { pointer, value }` and the pointer resolves to a
`FunctionArgument(i)` in `pointer_args`:

```rust
Statement::Store { pointer, value } => {
    match &ctx.func.expressions[*pointer] {
        Expression::FunctionArgument(i) if ctx.pointer_args.contains_key(i) => {
            let base_ty_h = ctx.pointer_args[i];
            let base_inner = &ctx.module.types[base_ty_h].inner;
            let ir_tys = naga_type_to_ir_types(base_inner)?;
            let addr = ctx.arg_vregs_for(*i)?[0];
            let srcs = ctx.ensure_expr_vec(*value)?;
            for (offset_idx, src) in srcs.iter().enumerate() {
                let off_vreg = /* compute addr + offset_idx * 4 */;
                ctx.fb.push(Op::Store { addr: off_vreg, src: *src });
            }
            Ok(())
        }
        // ...existing cases (LocalVariable)...
    }
}
```

### `lower_stmt.rs` — Call site plumbing

In `lower_user_call`, for each argument that targets a pointer-typed parameter
in the callee:

```rust
for (i, arg_h) in arguments.iter().enumerate() {
    let callee_arg = &callee_func.arguments[i];
    let callee_inner = &ctx.module.types[callee_arg.ty].inner;

    if let TypeInner::Pointer { base, .. } = callee_inner {
        // 1. Resolve the caller's local variable for this argument
        let lv = resolve_arg_to_local(ctx, *arg_h)?;
        let local_vregs = ctx.resolve_local(lv)?;

        // 2. Allocate a slot in caller's frame
        let base_inner = &ctx.module.types[*base].inner;
        let ir_tys = naga_type_to_ir_types(base_inner)?;
        let slot = ctx.fb.alloc_slot(ir_tys.len() as u32 * 4);

        // 3. Copy-in: store current values to slot (for inout)
        let addr = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(Op::SlotAddr { dst: addr, slot });
        for (j, src) in local_vregs.iter().enumerate() {
            let off_addr = /* addr + j * 4 */;
            ctx.fb.push(Op::Store { addr: off_addr, src: *src });
        }

        // 4. Pass slot address as the argument
        arg_vs.push(addr);

        // 5. Record for copy-back after the call
        inout_copybacks.push((lv, slot, ir_tys));
    } else {
        // existing: pass by value
        let vs = ctx.ensure_expr_vec(*arg_h)?;
        arg_vs.extend_from_slice(&vs);
    }
}

// ...emit Op::Call...

// 6. Copy-back: load from slots into local vregs
for (lv, slot, ir_tys) in &inout_copybacks {
    let local_vregs = ctx.resolve_local(*lv)?;
    let addr = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::SlotAddr { dst: addr, slot: *slot });
    for (j, dst) in local_vregs.iter().enumerate() {
        let off_addr = /* addr + j * 4 */;
        ctx.fb.push(Op::Load { dst: *dst, addr: off_addr });
    }
}
```

### `naga_type_to_ir_types` — Handle Pointer types

In `lower_ctx.rs`, add a `Pointer` case to `naga_type_to_ir_types`:

```rust
TypeInner::Pointer { base, .. } => {
    // Pointer params are represented as a single i32 address
    Ok(smallvec![IrType::I32])
}
```

### `scan_param_argument_indices` — Skip pointer params

The existing `scan_param_argument_indices` optimization (which aliases
locals to argument vregs) must skip pointer-typed arguments, since those
locals will be backed by memory, not vregs.

### Address offset helper

For multi-scalar types behind a pointer, we need to compute
`addr + offset_idx * 4`. This can use:

```rust
let off_vreg = if offset_idx == 0 {
    addr
} else {
    let off = ctx.fb.alloc_vreg(IrType::I32);
    let imm_vreg = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::Iconst { dst: imm_vreg, imm: (offset_idx * 4) as i32 });
    ctx.fb.push(Op::Iadd { dst: off, lhs: addr, rhs: imm_vreg });
    off
};
```

## Tests

- `function/edge-inout-both.glsl` — primary test file
- Run full filetest suite to check for regressions

## Validate

```bash
cargo test -p lps-frontend -q
cargo test -p lps-wasm -q
scripts/filetests.sh function/edge-inout-both.glsl
scripts/filetests.sh function/
```
