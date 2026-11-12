# Phase 1: Fix JIT Tests

## Description

Update all JIT tests to use `DecimalFormat::Q32` instead of `DecimalFormat::Float`. The Float format is explicitly rejected in `compile_glsl_to_gl_module_jit()` and is not yet supported.

## Tests to Fix

- `exec::jit::tests::test_jit_int_literal`
- `exec::jit::tests::test_jit_int_addition`
- `exec::jit::tests::test_jit_float_literal`
- `exec::jit::tests::test_jit_bool_literal`

## Implementation

1. Update each test to use `DecimalFormat::Q32` instead of `DecimalFormat::Float`
2. For `test_jit_float_literal`: Convert expected float value to fixed-point format for comparison
3. Run tests to verify they pass

## Success Criteria

- All 4 JIT tests pass
- Tests compile without errors
- No warnings introduced
