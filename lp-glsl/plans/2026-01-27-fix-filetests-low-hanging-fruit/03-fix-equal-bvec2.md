# Phase 3: Fix equal() Function for bvec2 Arguments

## Description

Fix the `equal()` builtin function to correctly handle bvec2 arguments in nested calls. The test `test_vec2_equal_function_in_expression()` fails when calling `equal(equal(a, b), equal(b, c))` where the inner calls return bvec2.

## Changes

### `lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/builtins/relational.rs`

- **`builtin_equal()` function**: Investigate and fix bvec2 argument handling
  - Check how bvec2 arguments are passed and compared
  - Verify the comparison logic works correctly for boolean vectors
  - Ensure return type is correct (bvec2 when comparing bvec2)
  - Test nested calls: `equal(equal(a, b), equal(b, c))`

## Investigation Steps

1. Run failing test with debug output to see what's happening
2. Check how bvec2 values are represented in Cranelift IR
3. Verify comparison logic in `builtin_equal()` handles boolean vectors correctly
4. Check if there's a type mismatch or incorrect comparison operation

## Success Criteria

- `equal()` correctly handles bvec2 arguments
- Nested calls work correctly: `equal(equal(a, b), equal(b, c))`
- Test `test_vec2_equal_function_in_expression()` passes
- Test `test_ivec2_equal_function_in_expression()` passes (if affected)
- No regressions in other equal() tests

## Implementation Notes

- bvec2 values are stored as i8 in StructReturn
- Comparison should use integer comparison (icmp) for boolean vectors
- Verify the return type is correctly determined for bvec2 arguments
- Check if there's an issue with how boolean values are compared
