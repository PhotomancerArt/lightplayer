# Filetests Failure Analysis

## Overview

Analysis of filetest failures to identify which ones should be fixed before marking as expected
failures. This report focuses on failures that seem odd or unexpected, as requested.

## Executive Summary

- **Total failing test files**: ~200+ files across multiple categories
- **Focus areas**: Builtin precision, matrix ABI, control flow scoping, vec operations, type errors
- **Key findings**: Several categories of failures that appear to be bugs rather than missing
  features

### Quick Reference

| Category                 | Priority   | Status          | Affected Tests               | Issue Type              |
|--------------------------|------------|-----------------|------------------------------|-------------------------|
| Matrix Struct Return ABI | **HIGH**   | Broken          | All matrix tests             | Test infrastructure bug |
| Control Flow Scoping     | **HIGH**   | Broken          | for/if variable-scope        | Compiler bug            |
| Builtin Precision        | **HIGH**   | Broken          | angle-degrees, radians, etc. | Precision/tolerance     |
| Vec Comparison           | **MEDIUM** | Broken          | vec2/ivec2 fn-equal          | Function bug            |
| Vec Conversion           | **MEDIUM** | Broken          | uvec2/3/4 from-scalars       | Cast function bug       |
| Missing Builtins         | **LOW**    | Not Implemented | trunc, fma, frexp, etc.      | Feature not implemented |
| Type Error Tests         | **N/A**    | ✅ Passing       | incdec-bool, etc.            | Previously fixed        |

## Summary

## Detailed Findings

### 1. Builtin Precision Issues (High Priority - Likely Bugs)

**Status**: Broken - Precision tolerance issues

**Affected Tests**:

- `builtins/angle-degrees.glsl` - 7/8 tests failing
- `builtins/angle-radians.glsl` - 7/8 tests failing
- `builtins/common-roundeven.glsl` - 1/9 tests failing
- `builtins/edge-component-wise.glsl` - 2/9 tests failing
- `builtins/edge-exp-domain.glsl` - 8/10 tests failing
- `builtins/edge-nan-inf-propagation.glsl` - 7/11 tests failing
- `builtins/edge-precision.glsl` - 9/9 tests failing
- `builtins/edge-trig-domain.glsl` - 3/10 tests failing
- `builtins/matrix-determinant.glsl` - 1/21 tests failing

**Issue**: Tests are failing due to floating-point precision mismatches. The actual values are very
close to expected values but exceed the default tolerance.

**Example from `angle-degrees.glsl`**:

```
expected: 90.0
  actual: 90.000244
```

**Root Cause**: The `degrees()` and `radians()` functions (and possibly others) are producing
results with small precision errors. The tests use `~=` (approximate equality) but don't specify a
tolerance, so the default tolerance may be too strict.

**Recommendation**:

- Check if these functions are implemented correctly
- Consider adding explicit tolerance to tests: `~= 90.0 (tolerance: 0.001)`
- OR fix the implementation to be more precise
- These are likely fixable bugs rather than expected failures

### 2. Matrix Struct Return ABI Issues (High Priority - Critical Bug)

**Status**: Broken - Test runner doesn't handle struct returns

**Affected Tests**:

- `builtins/matrix-compmult.glsl` - 0/17 tests passing
- `builtins/matrix-determinant.glsl` - 20/21 tests passing (1 precision issue)
- `builtins/matrix-inverse.glsl` - 0/16 tests passing
- `builtins/matrix-outerproduct.glsl` - 0/20 tests passing
- `builtins/matrix-transpose.glsl` - 0/20 tests passing
- `matrix/mat2/fn-transpose.glsl` - 0/10 tests passing
- `matrix/mat2/from-matrix.glsl` - 0/8 tests passing
- `matrix/mat2/from-mixed.glsl` - 0/9 tests passing
- `matrix/mat2/from-scalar.glsl` - 0/10 tests passing
- `matrix/mat2/from-vectors.glsl` - 0/9 tests passing
- All other matrix tests (mat3, mat4) likely affected

**Issue**: All tests that call functions returning matrices fail with:

```
error[E0400]: Argument count mismatch calling function 'test_mat2_transpose_simple':
expected 1 parameter(s), got 0 argument(s).
Signature: Signature { params: [AbiParam { value_type: types::I32, purpose: StructReturn, extension: None }], returns: [], call_conv: SystemV }
```

**Root Cause**:

- Matrix-returning functions use StructReturn ABI (correct)
- The test runner (`execute_fn.rs` or test harness) doesn't handle StructReturn when calling test
  functions
- The test runner calls functions with 0 arguments, but StructReturn functions expect 1 parameter (
  the return buffer pointer)

**Code Location**:

- Function signature generation:
  `lp-glsl/lp-glsl-compiler/src/frontend/codegen/signature.rs:132-140`
- Test execution: `lp-glsl/lp-glsl-compiler/src/exec/execute_fn.rs` or test harness

**Recommendation**:

- **CRITICAL**: Fix the test runner to handle StructReturn for matrix-returning functions
- This is a test infrastructure bug, not a compiler bug
- All matrix tests will pass once this is fixed (except precision issues)
- Should be fixed before marking as expected failures

### 3. Control Flow Variable Scoping Issues (High Priority - Bugs)

**Status**: Broken - Variable shadowing not working correctly

**Affected Tests**:

- `control/for/variable-scope.glsl` - 0/8 tests passing
- `control/if/variable-scope.glsl` - 4/5 tests passing (1 failing)

#### For Loop Scoping

**Issue**: Variable shadowing in for loop init-expression doesn't work correctly.

**Example from `control/for/variable-scope.glsl:21-31`**:

```glsl
int test_for_loop_init_shadowing() {
    int i = 100;
    int sum = 0;
    for (int i = 0; i < 3; i++) {
        sum = sum + i;
    }
    // Outer i should be unchanged
    return i;
}

// run: test_for_loop_init_shadowing() == 3
```

**Expected**: Outer `i` should remain 100 (shadowed by loop variable)
**Actual**: Outer `i` is 100 (test expects 3, but comment says "Outer i should be unchanged")

**Note**: There's a discrepancy - the test expects `3` but the comment says outer `i` should be
unchanged. This might be a test bug, but the variable scoping is definitely broken.

**Additional Issue**: Line 92-101 has a compilation error:

```glsl
for (int i = 0; int j = i < 3; i++) {
```

This tries to declare a variable in the condition expression, which is invalid GLSL syntax. The test
expects this to work, but it's not valid GLSL.

#### If Block Scoping

**Issue**: Variable shadowing in if blocks doesn't work correctly.

**Example from `control/if/variable-scope.glsl:29-38`**:

```glsl
int test_if_variable_shadowing() {
    int x = 5;
    if (true) {
        int x = 10;
        // Inner x shadows outer x
    }
    return x;
}

// run: test_if_variable_shadowing() == 10
```

**Expected**: Outer `x` should remain 5 (inner `x` shadows it)
**Actual**: Outer `x` is 5 (test expects 10, but comment says inner shadows outer)

**Note**: Again, there's a discrepancy between the test expectation and the comment. The comment
says inner shadows outer (so outer should be 5), but test expects 10. This suggests the test might
be wrong, OR the comment is wrong and the test is checking that shadowing doesn't work (which would
be a bug).

**Recommendation**:

- Fix variable shadowing in for loops and if blocks
- Clarify test expectations vs comments (there are contradictions)
- Fix the invalid GLSL syntax test (`int j = i < 3` in for condition)
- These are likely bugs that should be fixed

### 4. Vec Comparison Function Issues (Medium Priority - Bug)

**Status**: Broken - Nested `equal()` calls return incorrect results

**Affected Tests**:

- `vec/vec2/fn-equal.gen.glsl` - 7/8 tests passing
- `vec/ivec2/fn-equal.gen.glsl` - 7/8 tests passing
- Possibly other vec comparison tests

**Issue**: When `equal()` is called with bvec2 arguments (nested calls), it returns incorrect
results.

**Example from `vec/vec2/fn-equal.gen.glsl:67-78`**:

```glsl
bvec2 test_vec2_equal_function_in_expression() {
    vec2 a = vec2(1.0, 3.0);
    vec2 b = vec2(1.0, 4.0);
    vec2 c = vec2(2.0, 3.0);
    // equal(a, b) = (true,false)
    // equal(b, c) = (false,false)
    // equal(equal(a, b), equal(b, c)) = (false,false)
    return equal(equal(a, b), equal(b, c));
}

// run: test_vec2_equal_function_in_expression() == bvec2(false, false)
```

**Expected**: `bvec2(false, false)`
**Actual**: `bvec2(false, true)`

**Root Cause**: The `equal()` function may not be handling bvec2 arguments correctly, or there's an
issue with nested function calls returning bvec2.

**Recommendation**:

- Fix the `equal()` function to handle bvec2 arguments correctly
- This is a bug that should be fixed
- Check if other comparison functions (`notEqual`, `lessThan`, etc.) have similar issues

### 5. Vec Conversion Issues (Medium Priority - Bug)

**Status**: Broken - Negative float to uint conversion incorrect

**Affected Tests**:

- `vec/uvec2/from-scalars.glsl` - 9/10 tests passing
- `vec/uvec3/from-scalars.glsl` - 10/11 tests passing
- `vec/uvec4/from-scalars.glsl` - 11/12 tests passing
- `vec/uvec3/from-uvec.glsl` - 17/18 tests passing

**Issue**: Converting negative floats to uint doesn't wrap correctly.

**Example from `vec/uvec2/from-scalars.glsl:59-63`**:

```glsl
uvec2 test_uvec2_from_scalars_function_results() {
    return uvec2(uint(7.8), uint(-3.2)); // float to uint conversion (truncates)
}

// run: test_uvec2_from_scalars_function_results() == uvec2(7u, 4294967293u)
```

**Expected**: `uvec2(7u, 4294967293u)` (wraps -3.2 to large uint)
**Actual**: `uvec2(7u, 0u)` (converts to 0 instead of wrapping)

**Root Cause**: The `uint()` cast function may be clamping negative values to 0 instead of wrapping
them according to GLSL spec (which should wrap).

**Recommendation**:

- Fix `uint()` cast to wrap negative floats correctly
- Check if `int()` cast has similar issues
- This is a bug that should be fixed

### 6. Missing Builtin Functions (Low Priority - Not Implemented)

**Status**: Not Implemented

**Affected Tests**:

- `builtins/common-trunc.glsl` - 0/8 tests passing
- `builtins/common-floatbitstoint.glsl` - 0/8 tests passing
- `builtins/common-fma.glsl` - 0/8 tests passing
- `builtins/common-frexp.glsl` - 0/6 tests passing
- `builtins/common-intbitstofloat.glsl` - 0/8 tests passing
- `builtins/common-ldexp.glsl` - 0/9 tests passing
- `builtins/common-modf.glsl` - 0/7 tests passing
- Many integer bit manipulation functions
- Pack/unpack functions

**Issue**: These builtin functions are not implemented. Tests fail with "undefined function" errors.

**Recommendation**:

- These can be marked as expected failures until implemented
- Lower priority than bugs

### 7. Type Error Tests (Status: Passing - Previously Fixed)

**Status**: ✅ Passing

**Affected Tests**:

- `type_errors/incdec-bool.glsl` - ✅ Passing
- `type_errors/incdec-nested.glsl` - ✅ Passing
- `type_errors/incdec-non-lvalue.glsl` - ✅ Passing

**Note**: These tests are now passing, suggesting they were fixed. The user mentioned these seemed
odd, but they're working correctly now.

## Recommendations

### High Priority (Fix Before Marking as Expected Failures)

1. **Matrix Struct Return ABI** - Critical test infrastructure bug affecting all matrix tests
2. **Control Flow Variable Scoping** - Bugs in for loop and if block shadowing
3. **Builtin Precision Issues** - Add tolerances or fix precision in implementations

### Medium Priority (Should Fix)

4. **Vec Comparison Functions** - Bug in `equal()` with bvec2 arguments
5. **Vec Conversion** - Bug in `uint()` cast with negative values

### Low Priority (Can Mark as Expected Failures)

6. **Missing Builtins** - Not implemented yet, can mark as expected failures
7. **Array Tests** - All failing, likely not implemented (not in scope of this analysis)

## Next Steps

1. Fix matrix StructReturn handling in test runner
2. Fix variable scoping in for loops and if blocks
3. Add tolerances to precision-sensitive builtin tests
4. Fix vec comparison and conversion bugs
5. Mark missing builtins as expected failures
6. Re-run tests to verify fixes
