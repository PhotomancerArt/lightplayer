# Phase 8: Tests + Cleanup

## Scope

Write wasmtime smoke tests exercising the full GLSL → LPIR → WASM
pipeline. Fix any issues found. Run clippy and format. Verify existing
smoke tests pass.

## Tests

### Update `tests/smoke.rs`

The existing smoke tests call `glsl_wasm()` which now goes through the
LPIR pipeline. They should pass without changes to the test code itself
(the public API is unchanged).

Tests to verify still pass:
- `float_literal_return` — Q32 literal emission
- `float_add` — float mode (will error now since float mode unsupported;
  update to Q32 or mark `#[ignore]`)
- `int_add_typed` — integer arithmetic (Q32 default)
- `multiple_functions_exported` — multi-function modules
- `q32_add` — Q32 addition
- `q32_chained_float_compare_and` — Q32 control flow + comparisons
- `q32_chained_float_compare_or` — Q32 logical or
- `q32_triple_float_compare_and` — nested logical chains

Update `float_literal_return` and `float_add` to use Q32 mode (or mark
`#[ignore]` since float mode is out of scope).

### New smoke tests

Add tests for ops not covered by existing tests:

**Arithmetic:**
- `q32_subtract` — float subtraction
- `q32_multiply` — float multiplication
- `q32_divide` — float division
- `q32_negate` — float negation
- `q32_int_modulo` — integer modulo

**Control flow:**
- `q32_if_else` — conditional return
- `q32_for_loop` — loop with accumulation
- `q32_while_loop` — while-style loop
- `q32_nested_loops` — nested loop with break

**Math builtins (if builtins WASM is available):**
- `q32_abs` — `abs(x)` via `Fabs`
- `q32_min_max` — `min(a, b)` and `max(a, b)`
- `q32_floor_ceil` — `floor(x)` and `ceil(x)`
- `q32_mix` — `mix(a, b, t)` (inline decomposition)
- `q32_clamp` — `clamp(x, lo, hi)`
- `q32_step` — `step(edge, x)`

**User function calls:**
- `q32_call_user_func` — call between two user functions
- `q32_call_chain` — A calls B calls C

**Casts:**
- `q32_float_to_int` — `int(x)` from float
- `q32_int_to_float` — `float(x)` from int

### Test helpers

Keep the existing `run_q32_f32` and `run_q32_f32_0` helpers. Remove or
`#[ignore]` the `run_f32` helper (float mode not supported).

Add:
```rust
fn run_q32_i32(source: &str, func_name: &str, args_i32: &[i32]) -> i32 {
    // Compile, instantiate, call with i32 args, return i32
}
```

## Cleanup

### Warnings
```
cargo clippy -p lp-glsl-wasm -- -D warnings
```

### Formatting
```
cargo +nightly fmt -p lp-glsl-wasm
```

### Dead code audit

Verify no remnants of the old emitter remain:
- No references to `locals.rs`, `emit_vec.rs`, `lpfx.rs`, `types.rs`
- No unused imports of `naga::*` types
- No unused `wasm-encoder` instruction imports

### Cross-crate check

```
cargo check --workspace
cargo test -p lp-glsl-wasm
```

### Filetest spot-check

Run a few scalar filetests manually to verify:
```
cargo test -p lp-glsl-filetests -- scalar::float::op_add
cargo test -p lp-glsl-filetests -- scalar::int::op_add
cargo test -p lp-glsl-filetests -- scalar::bool::ctrl_if
```

Full filetest validation is Stage VI, but a spot-check here catches
obvious issues early.

## Validate

```
cargo test -p lp-glsl-wasm
cargo clippy -p lp-glsl-wasm -- -D warnings
cargo +nightly fmt -p lp-glsl-wasm -- --check
```

All smoke tests pass. The LPIR → WASM pipeline works end-to-end.
