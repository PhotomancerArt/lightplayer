# Phase 5: Test Maintenance and Cleanup

## Scope

Fix stale test annotations, broken test directives, and incorrect expected
values. Run full filetest suite to verify all P0 fixes. Final cleanup.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation

### 1. Strip stale `@unimplemented` markers

```bash
scripts/glsl-filetests.sh --fix
```

This removes `@unimplemented(backend=wasm)` from tests that now pass on
`wasm.q32`. Affected files include:

- `control/for/complex-condition.glsl`
- `operators/predec-scalar-float.glsl`
- `operators/predec-scalar-int.glsl`
- `operators/predec-vec2.glsl`
- `operators/preinc-mat3.glsl`
- `operators/preinc-vec2.glsl`
- `operators/incdec-scalar.glsl`
- `operators/incdec-vector.glsl`
- `operators/incdec-matrix.glsl`

### 2. Fix commented-out function references

**`function/overload-ambiguous.glsl`:**
The run directive for `test_overload_ambiguous_promotions` references a
function inside a `/* */` block. Comment out the run directive too:

```
// @unimplemented()
// @unimplemented(backend=wasm)  // function in commented block
// run: test_overload_ambiguous_promotions() ~= 5.0
```

→ Already commented. The issue is likely the parser still picking it up.
Investigate whether the `// run:` inside a `/* */` block is parsed. If so,
add a blank line or move the directive inside the comment block.

**`function/recursive-static-error.glsl`:**
Same pattern — `test_recursive_deep` is in a `/* */` block but its run
directive is outside. Move it inside or comment it differently.

### 3. Re-bless rainbow expected values

```bash
scripts/glsl-filetests.sh debug/rainbow.glsl --target cranelift.q32
```

Capture the actual values from cranelift.q32 output and update the expected
values in `debug/rainbow.glsl`. The current expectations are all-zero
placeholders.

### 4. Fix `float(INT_MAX)` expectation

In `scalar/float/from-int.glsl`, update:

```glsl
// run: test_float_from_int_large() ~= 32767.0
```

to:

```glsl
// run: test_float_from_int_large() ~= 32768.0
```

`ItofS(2147483647)` saturates to Q32_MAX ≈ 32767.99998 which rounds to
32768.0 in f32. This is correct behavior.

### 5. Full filetest run

```bash
scripts/glsl-filetests.sh
```

Verify all previously-failing tests now pass (or are correctly annotated).

## Cleanup

- Grep the git diff for TODOs, debug prints, temporary code.
- Run `cargo +nightly fmt` on all changed files.
- Run `cargo clippy -p lps-wasm -p lps-frontend -- -D warnings`.

## Plan cleanup

- Write `summary.md` with completed work.
- Move plan files to `docs/plans-done/`.

## Commit

```
fix(lps): filetest failures — continue depth, bool casts, prototypes, inout

- Fix WASM continue branch depth in nested constructs (control.rs)
- Handle As expressions with Bool target type (expr_scalar.rs)
- Support function forward declarations via two-pass lowering
- Implement slot-based inout/out parameter passing matching Cranelift ABI
- Strip stale @unimplemented markers, fix test expectations
```

## Validate

```bash
cargo test -p lps-wasm -q
cargo test -p lps-frontend -q
cargo +nightly fmt --check -p lps-wasm -p lps-frontend
cargo clippy -p lps-wasm -p lps-frontend -- -D warnings
scripts/glsl-filetests.sh
```
