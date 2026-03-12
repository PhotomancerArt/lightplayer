# Phase 9: Tests and Validation

## What to test

This plan changes dispatch logic, not math. The correctness of the Q32
math itself is tested in Plan B (unit tests) and validated end-to-end
in Plan E. Here we test that:

1. The right function is called (Q32 builtin vs float libcall)
2. The right signature is used (i32 vs f32 params/returns)
3. Inline expansions (isinf, isnan) emit correct Q32 instructions

## Test strategy

### Unit tests for helpers

Test `get_q32_math_builtin` mapping coverage — verify every float
libcall name resolves to the expected BuiltinId. These are mostly
covered by existing tests in `converters/math.rs::tests`
(`test_map_testcase_to_builtin_*`), but a sanity check from the
helpers side is worthwhile.

### Integration: filetests

The filetests are the primary validation:

```bash
scripts/glsl-filetests.sh
```

Since Plan C doesn't wire Q32Strategy into the pipeline (that's Plan D),
the filetests should pass unchanged — the float path is untouched.

### Smoke test for Q32 dispatch (deferred to Plan D)

A true integration test of Q32 dispatch requires the pipeline to be
connected. This happens in Plan D. At that point, compile a shader with
Q32 mode and verify:
- Trig calls go to `__lp_q32_sin`, not `sinf`
- LPFX calls go to `__lpfx_*_q32`, not `__lpfx_*_f32`
- `isinf` is expanded inline, not emitted as a call

## Validate

```bash
cargo check -p lp-glsl-compiler --features std
cargo test -p lp-glsl-compiler --features std
scripts/glsl-filetests.sh
```
