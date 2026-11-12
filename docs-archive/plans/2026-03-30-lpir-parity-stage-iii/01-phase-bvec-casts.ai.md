# Phase 1: Bvec to Numeric Vector Casts

## Scope of phase

Fix `As` and `Compose` lowering in `lps-frontend/src/lower_expr.rs` to correctly handle bvec to
numeric vector casts (`vec2(bvec2(...))`). Current failure: "assignment component count 1 vs 2".

## Code organization reminders

- Keep related conversion logic grouped in `lower_expr_vec_uncached`
- Place helper functions at the bottom of the module
- Use explicit pattern matching for the bvec -> vector case

## Implementation details

### The problem

When GLSL does `vec2(bvec2(true, false))`, Naga produces either:

- `As { expr: bvec_expr, kind: Float }` (cast)
- `Compose { ty: vec2_ty, components: [bvec_expr] }` (construction)

Current lowering produces a single scalar instead of a 2-component vector.

### The fix

For bvec -> numeric vector conversions, lower to component-wise select:

- `true` -> `1.0` (Q32: `65536` / `0x10000`)
- `false` -> `0.0` (Q32: `0`)

```rust
// In lower_expr_vec_uncached, handle As for bvec -> vector
Expression::As { expr, kind, .. } => {
    let src_inner = expr_type_inner(ctx.module, ctx.func, *expr)?;
    match (&src_inner, kind) {
        // bvec -> float/uint/int vector
        (TypeInner::Vector { size, scalar: Scalar { kind: ScalarKind::Bool, .. } }, target_kind)
            if *target_kind != ScalarKind::Bool =>
        {
            // Lower source bvec, then component-wise: true->1.0, false->0.0
            let src_vs = lower_expr_vec(ctx, *expr)?;
            let n = vector_size_usize(*size);
            let target_scalar = match target_kind {
                ScalarKind::Float => Scalar { kind: ScalarKind::Float, width: 4 },
                ScalarKind::Sint => Scalar { kind: ScalarKind::Sint, width: 4 },
                ScalarKind::Uint => Scalar { kind: ScalarKind::Uint, width: 4 },
                _ => return Err(LowerError::UnsupportedExpression(...)),
            };
            // Generate per-component select into new vregs
            let mut out = VRegVec::new();
            for i in 0..n {
                let src_comp = src_vs[i];
                let dst = ctx.fb.alloc_vreg(naga_scalar_to_ir_type(target_scalar.kind)?);
                // Select: if src_comp then 1.0 else 0.0
                // For Q32 float: 1.0 = 0x10000 (65536)
                // Implementation: use existing select pattern or arithmetic
                ctx.fb.push(...); // generate select
                out.push(dst);
            }
            Ok(out)
        }
        _ => /* existing As handling */
    }
}
```

### Similar handling for Compose

When `Compose { ty, components }` has a bvec as a component and the target type is a numeric vector,
ensure the component is expanded correctly.

## Validate

```bash
# Run bvec cast tests
cd /Users/yona/dev/photomancer/lp2025
./scripts/filetests.sh --target jit.q32 "vec/bvec2/to-float.glsl" "vec/bvec2/to-int.glsl"

# Verify no regressions
./scripts/filetests.sh --target jit.q32 "vec/bvec2/"

# Check compilation
cargo check -p lps-frontend
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```
