# Phase 2: Bool Casts and Ternary Conversion

## Scope

Fix `As` expressions targeting Bool type (`bool(x)` constructors) and the
related ternary implicit conversion bug where Q32 float values leak through
as raw integers.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation

### `lp-shader/lps-frontend/src/expr_scalar.rs`

The current `As` handling (in `lower_expr.rs` or `expr_scalar.rs`) rejects
casts where the target type byte width != 4. Bool is 1 byte in Naga.

Add a case for Bool target **before** the byte-width check:

```rust
// As with Bool target: compare-not-equal-to-zero
if target_scalar.kind == ScalarKind::Bool {
    let src = ctx.ensure_expr(expr)?;
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    let zero = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::Iconst { dst: zero, imm: 0 });
    ctx.fb.push(Op::Ine { dst, lhs: src, rhs: zero });
    return Ok(smallvec![dst]);
}
```

For Q32 mode, float zero is `0i32`, so `Ine(src, 0)` works for all source
types (float-as-Q32, int, uint, bool). `bool(bool)` is technically identity
but `Ine(b, 0)` is equivalent and safe.

### Ternary conversion (Bug 3)

The `test_ternary_float_to_int_conversion` failure produces 675020 (raw Q32
of 10.3). This means the `As(float → int)` wrapping the ternary result isn't
applying the Q32→int truncation.

This is likely a separate `As` code path issue: the existing `As` handler for
`float → int` may work for direct casts but fail when the source expression
is a ternary (Select). After fixing the Bool case, check:

1. Does `test_ternary_float_to_int_conversion` now pass?
2. If not, trace the Naga expression tree:
    - The ternary result should be `Expression::Select { ... }` with float type.
    - The `int result = ...` assignment wraps it in `As(float → Sint)`.
    - Verify the `As(float → Sint)` path emits `Op::FtoiSatS` in Q32 mode.

### Tests

Filetests to validate:

- `scalar/bool/from-bool.glsl`
- `scalar/bool/from-float.glsl`
- `scalar/bool/from-int.glsl`
- `scalar/bool/from-uint.glsl`
- `control/ternary/type_conversions.glsl`

## Validate

```bash
cargo test -p lps-frontend -q
scripts/glsl-filetests.sh scalar/bool/
scripts/glsl-filetests.sh control/ternary/type_conversions.glsl
```
