# Phase 2: Math gaps

## Scope of phase

Add `floor` and `fract` for Q32 as inline builtins. Verify `atan(y,x)`, `cos`, and `exp` work
through the existing import path. After this phase, every standard GLSL builtin used in
`rainbow.shader/main.glsl` compiles correctly (LPFX calls are separate — phase 3).

## Code organization reminders

- Inline builtins go in `builtin_inline.rs` with the existing pattern.
- Add new entries to `q32_builtin_import_suppressed` for any builtin that should not generate an
  import.
- Tests first in test modules; helpers at bottom.

## Implementation details

### 1. Inline `floor` Q32

File: `lp-shader/lp-glsl-wasm/src/codegen/expr/builtin_inline.rs`

Add `("floor", 1)` to the `try_emit_inline_builtin` match and to `q32_builtin_import_suppressed`.

Implementation matches Cranelift (`numeric.rs` line 444–448): arithmetic right shift by 16 discards
fractional bits, left shift by 16 restores position.

```rust
fn emit_floor_q32_component(sink: &mut InstructionSink) {
    sink.i32_const(16);
    sink.i32_shr_s();
    sink.i32_const(16);
    sink.i32_shl();
}
```

For vectors: evaluate arg, store components to scratch locals, apply per-component, leave results on
stack. Same pattern as `emit_sign`.

The Float path can use `f32.floor` directly.

### 2. Inline `fract` Q32

Existing `emit_fract` handles Float mode only and errors on Q32. Extend it to support Q32 using
`x - floor(x)`, matching Cranelift (`builtins/common.rs` line 418–431).

Q32 implementation per component:

1. Duplicate the value (or load from scratch local)
2. Apply floor (shift right 16, shift left 16)
3. Subtract: `i32.sub`

This requires scratch locals for the original value since floor is destructive. Use the existing
`binary_op_i32_base` scratch slots (same pattern as other compound inlines like `sign`).

Add `("fract", 1)` to `q32_builtin_import_suppressed`.

### 3. Verify `atan(y, x)` path

`glsl_q32_math_builtin_id("atan", 2)` returns `Some(LpQ32Atan2)` (confirmed in generated tests). The
existing FunCall dispatch checks `is_builtin_function("atan")` (true) and
`glsl_q32_math_builtin_id("atan", 2).is_some()` (true), so it routes to `emit_q32_math_libcall`. The
WASM import type for `LpQ32Atan2` is `(i32, i32) -> i32`.

This should work with no code changes. Write a test to confirm: compile a shader with
`atan(1.0, 0.5)` and verify it produces a valid import + call.

### 4. Verify `cos` and `exp`

Same situation as `sin` (already verified). `glsl_q32_math_builtin_id("cos", 1)` → `LpQ32Cos`,
`glsl_q32_math_builtin_id("exp", 1)` → `LpQ32Exp`. Both have WASM import type `(i32) -> i32`.

Write compile-and-link tests in `q32_builtin_link.rs` or `basic.rs` for `cos` and `exp`, similar to
the existing `sin(1.0)` test.

### 5. Tests

In `lp-glsl-wasm/tests/basic.rs`:

- `test_q32_floor_inline` — compile shader with `floor(1.7)`, verify no import emitted (inline path)
- `test_q32_fract_inline` — compile shader with `fract(1.7)`, verify no import emitted
- `test_q32_floor_vec3` — floor on a vec3

In `lp-glsl-wasm/tests/q32_builtin_link.rs` (or basic.rs compile-only):

- `test_q32_atan2_compiles` — `atan(1.0, 0.5)` compiles and produces correct import
- `test_q32_cos_compiles` — `cos(1.0)` compiles
- `test_q32_exp_compiles` — `exp(1.0)` compiles

## Validate

```bash
cd lp-glsl && cargo test -p lp-glsl-wasm
cargo +nightly fmt
```

No new warnings. `floor` and `fract` should appear in `q32_builtin_import_suppressed`, not in the
import section.
