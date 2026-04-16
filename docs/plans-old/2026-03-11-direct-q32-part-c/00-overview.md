# Plan C: Numeric-Aware Builtin Dispatch

Part of the direct-Q32 design (docs/designs/2026-03-11-direct-q32).
Depends on Plan A (NumericStrategy trait + FloatStrategy) being complete.
Can be worked in parallel with Plan B (Q32Strategy inline ops).

## Goal

Make the builtin function dispatch paths numeric-mode-aware. When
compiling in Q32 mode, builtins call Q32 variants directly instead of
emitting float calls that the transform rewrites later.

This plan also resolves the `todo!()` stubs left by Plan B — saturating
add/sub/mul/div and sqrt — by giving the strategy a way to call builtins.

## Current state

Three call paths exist in the codegen, all hardcoded to float:

1. **Math libcalls** — `get_math_libcall("sinf")` → TestCase("sinf") with
   f32 signature. Transform rewrites to `__lp_q32_sin`.
2. **LPFX functions** — `get_lpfx_testcase_call(func, float_impl, ...)` →
   TestCase with float sig. Transform rewrites to q32_impl.
3. **Inline builtins** — `sign`, `fract`, `isinf`, `isnan` use
   `emit_float_*` helpers. Transform rewrites calls inline.

## Approach

Add `DecimalFormat` awareness to `CodegenContext` (it already has
`NumericMode`). Each call path branches on the format:

- **Float**: keep current behavior (TestCase libcalls, float instructions).
- **Q32**: emit Q32 builtin calls via `gl_module.get_builtin_func_ref()`
  or inline Q32 expansions.

## Scope

| File | Changes |
|------|---------|
| `context.rs` | Add `is_q32()` convenience method (derives from `NumericMode`) |
| `builtins/helpers.rs` | Numeric-aware `get_math_libcall` / `get_math_libcall_2arg` |
| `backend/builtins/` | Move `map_testcase_to_builtin` here from transform |
| `builtins/trigonometric.rs` | Use new helpers for all trig functions |
| `builtins/common.rs` | Q32 branches for inline builtins; use new helpers for libcalls |
| `lpfx_fns.rs` | Select float vs Q32 variant based on numeric mode |
| `numeric.rs` | Fill in Plan B `todo!()` stubs for saturating ops + sqrt |

## Non-scope

- Wiring Q32Strategy into the compilation pipeline (Plan D)
- Vector/matrix builtins (geometric, matrix) — these compose from scalar
  ops that already go through the strategy, so they work automatically
- Relational builtins — integer-only, unaffected by numeric mode

## Phases

1. Add `is_q32()` helper on CodegenContext (derives from NumericMode)
2. Move `map_testcase_to_builtin` to `backend/builtins/`
3. Numeric-aware math libcall helpers
4. Update trig builtins
5. Update common builtins (libcall-based)
6. Inline Q32 builtins (sign, fract, isinf, isnan)
7. LPFX dispatch
8. Fill in Q32Strategy builtin stubs (saturating ops, sqrt)
9. Tests + validation
