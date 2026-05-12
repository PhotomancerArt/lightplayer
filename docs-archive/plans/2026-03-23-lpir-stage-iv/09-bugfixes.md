# Phase 9: Bug Fixes

## Scope

Fix two lowering bugs discovered during Phase 7/8 testing, then fill in
the remaining test coverage from 07-tests.md.

## Bug 1: Local variable init not emitted

**Symptom**: `interp_loop_sum` fails LPIR validation with "used before
definition" for loop counter/accumulator vregs.

**Root cause**: Naga's GLSL frontend places constant initializers (e.g.
`int s = 0;`) into `LocalVariable.init` instead of emitting a
`Statement::Store` when the init expression is const and the declaration
is outside a loop. `LowerCtx::new` allocates vregs for locals but
ignores the `init` field, so those vregs never receive a defining op
before they are read.

**Fix** (in `lower_ctx.rs`, end of `LowerCtx::new`): build the full
`LowerCtx`, then iterate `func.local_variables` in arena order (skip
`param_aliases`). For each `var.init: Some(h)`, `h` refers to the function's `expressions` arena,
not
`global_expressions`. Call
`lower_expr::lower_expr(&mut ctx, h)` and emit `Op::Copy` into the local's
vreg. Const inits (literals, `Constant`, const-folded ops) are covered;
non-const inits still use `Statement::Store` from Naga and do not appear
in `var.init`.

Also remove the `#[ignore]` from `interp_loop_sum` once fixed.

**Scope**: `lower_ctx.rs` (+ `Op` import); tests.

## Bug 2: `Expression::As` rejects `convert: Some(4)`

**Symptom**: `int(x)` and `float(x)` casts fail with
`UnsupportedExpression("As with explicit byte convert")`.

**Root cause**: Naga's GLSL frontend always emits
`Expression::As { convert: Some(4) }` for scalar casts (4 = byte width
of the 32-bit target). The lowering rejects any `convert.is_some()`.
Since LPIR only has 32-bit scalars, `Some(4)` is always valid.

**Fix** (in `lower_expr.rs`, the `Expression::As` match arm):

Change:

```rust
if convert.is_some() {
    return Err(LowerError::UnsupportedExpression(...));
}
```

To:

```rust
if convert.is_some_and(|w| w != 4) {
    return Err(LowerError::UnsupportedExpression(...));
}
```

Then `Some(4)` falls through to `lower_as`, which already handles all
scalar-kind conversions (`FtoiSatS`, `ItofS`, etc.).

**Scope**: 1 line changed.

## Test coverage fill-in

After the two fixes, add the remaining tests from 07-tests.md that are
now unblocked:

### `lower_interp.rs` additions

- `interp_loop_sum` — remove `#[ignore]`, verify `sum(4) == 6`
- `interp_float_to_int` — `int f(float x) { return int(x); }`
- `interp_int_to_float` — `float f(int x) { return float(x); }`
- `interp_float_comparisons` — `<`, `<=`, `>`, `>=`, `==`, `!=`
- `interp_int_comparisons` — same for int
- `interp_bool_literal` — `bool f() { return true; }` → i32(1)
- `interp_nested_if` — nested if/else chains
- `interp_floor_ceil_trunc` — test all three
- `interp_min_max_float` — min/max
- `interp_min_max_int` — min/max for integers
- `interp_clamp` — clamp within and outside range
- `interp_sign` — positive, negative, zero
- `interp_step` — edge cases
- `interp_smoothstep` — polynomial shape
- `interp_fract` — fractional part
- `interp_fma` — `fma(a, b, c)`
- `interp_exp_log` — `exp(0)==1`, `log(1)==0`
- `interp_sin_cos` — `sin(0)≈0`, `cos(0)≈1`, `sin(π/2)≈1`

### `lower_print.rs` additions

- `print_loop` — verify `loop {` structure appears

## Validate

```
cargo test -p lps-frontend
cargo clippy -p lps-frontend -- -D warnings
cargo +nightly fmt -p lps-frontend -- --check
```

All tests pass, including the previously-ignored loop test and the
previously-failing cast tests. No new warnings.
