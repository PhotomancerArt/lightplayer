# Phase 7: Tests and Validation

## Unit tests

Add tests to the `mod tests` block in `numeric.rs`. These test the Q32
strategy methods in isolation using Cranelift's `FunctionBuilder` test
infrastructure.

### Test infrastructure

Each test needs a minimal Cranelift function with a `FunctionBuilder`.
Create a test helper:

```rust
fn with_q32_builder<F>(f: F)
where
    F: FnOnce(&Q32Strategy, &mut FunctionBuilder),
{
    // Create a minimal function, FunctionBuilderContext, FunctionBuilder
    // Call f with the strategy and builder
    // No need to compile — just verify the emitted instructions
}
```

### Tests per operation

Each Q32Strategy method should have at least one test verifying it emits
the expected instructions. For simple operations (neg, abs, min, max),
one test is sufficient. For mode-dependent operations (add, mul, div),
test each mode.

Priorities:
1. `emit_const` — verify `float_to_fixed16x16` is applied correctly
2. `emit_add` (wrapping) — verify `iadd`
3. `emit_mul` (wrapping) — verify the 5-instruction multiply sequence
4. `emit_div` (reciprocal) — verify the reciprocal sequence
5. `emit_cmp` — verify FloatCC → IntCC translation
6. `emit_from_sint` / `emit_to_sint` — verify shift + clamping
7. `map_signature` — verify F32 → I32 replacement

### Cross-validation (optional, lower priority)

Compile a simple function with both the Q32 transform and the Q32Strategy,
then compare the CLIF IR. This requires more infrastructure (a full
compilation pipeline mock) and is better suited for Plan D integration
testing.

## Validation

After all methods are implemented:

```bash
cargo check -p lp-glsl-compiler --features std
cargo test -p lp-glsl-compiler --features std -- numeric
```

The Q32Strategy doesn't affect compiler output yet (it's not wired into
the pipeline until Plan D), so `scripts/glsl-filetests.sh` should pass
unchanged — but run it as a sanity check that Plan A's FloatStrategy
wiring is still correct.

```bash
scripts/glsl-filetests.sh
```
