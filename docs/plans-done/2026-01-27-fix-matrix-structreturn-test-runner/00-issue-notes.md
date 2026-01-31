# Issue: Matrix Struct Return ABI - Test Runner

## Problem

All matrix-returning test functions fail with:

```
error[E0400]: Argument count mismatch calling function 'test_mat2_transpose_simple':
expected 1 parameter(s), got 0 argument(s).
Signature: Signature { params: [AbiParam { value_type: types::I32, purpose: StructReturn, extension: None }], returns: [], call_conv: SystemV }
```

## Affected Tests

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

## Root Cause Analysis

### Current State

1. **Function Signature Generation** (`signature.rs:132-140`):
    - Matrix-returning functions correctly use StructReturn ABI
    - StructReturn parameter is added as first parameter
    - Function returns void (StructReturn functions don't return values)

2. **Test Execution** (`execute_fn.rs:68-112`):
    - `execute_function()` handles matrix types by calling `call_mat()`
    - This should handle StructReturn correctly

3. **Error Location**:
    - Error occurs when test runner tries to call the function
    - Error message shows function signature expects 1 parameter (StructReturn pointer)
    - But test runner is calling with 0 arguments

### Investigation Needed

1. Check how `execute_function()` determines if a function uses StructReturn
2. Check if `call_mat()` in the `GlslExecutable` trait properly handles StructReturn
3. Verify signature detection logic in test harness
4. Check if there's a mismatch between how functions are compiled vs how they're called

### Key Files to Investigate

- `lp-glsl/lp-glsl-compiler/src/exec/execute_fn.rs` - Test execution entry point
- `lp-glsl/lp-glsl-compiler/src/frontend/codegen/signature.rs` - Signature generation
- `lp-glsl/lp-glsl-compiler/src/exec/emu.rs` - Emulator execution (check `call_mat()`
  implementation)
- `lp-glsl/lp-glsl-compiler/src/exec/jit.rs` - JIT execution (check `call_mat()` implementation)
- `lp-glsl/lp-glsl-filetests/src/test_run/run_detail.rs` - Test harness

## Expected Behavior

When calling a matrix-returning function:

1. Test runner should detect that function uses StructReturn
2. Allocate a buffer for the return value
3. Pass the buffer pointer as the first argument
4. Call the function with the StructReturn pointer
5. Read the result from the buffer

## Notes

- This is a test infrastructure bug, not a compiler bug
- The compiler correctly generates StructReturn signatures
- The issue is in how the test runner calls these functions
- Once fixed, all matrix tests should pass (except precision issues)
