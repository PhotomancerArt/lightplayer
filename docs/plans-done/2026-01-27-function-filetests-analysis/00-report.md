# Function-Related Filetests Analysis

## Overview

Analysis of 39 failing function-related filetests to categorize issues as:

- **Not Implemented**: Feature not yet implemented in the compiler
- **Broken**: Feature implemented but not working correctly
- **Bad Tests**: Tests that expect unsupported behavior (e.g., nested function definitions)

## Summary

- **Total failing tests**: 39 files
- **Tests passing**: 7/8 tests in `call-simple.glsl` (1 failing due to void)
- **Main categories**:
    1. Nested function definitions (parser limitation)
    2. Void return types (execution layer limitation)
    3. Out/InOut parameters (not implemented - pass by reference)
    4. Overload resolution (depends on nested functions)
    5. Forward declarations (needs verification)
    6. Various edge cases

## Detailed Findings

### 1. Nested Function Definitions (Parser Limitation)

**Status**: Not Implemented (Parser doesn't support)

**Affected Tests**:

- `return-scalar.glsl` - All tests fail with parse error: `expected '}', found f`
- `overload-resolution.glsl` - All tests fail (uses nested functions)
- `overload-ambiguous.glsl` - Likely fails (uses nested functions)
- `overload-same-name.glsl` - Likely fails (uses nested functions)
- `define-simple.glsl` - Test `test_define_simple_nested` fails

**Issue**: GLSL doesn't allow function definitions inside other functions, but these tests assume
it's supported. The parser encounters a function definition inside another function and fails with a
parse error.

**Example from `return-scalar.glsl`**:

```glsl
float test_return_float_simple() {
    float get_pi() {  // <-- Parser fails here
        return 3.14159;
    }
    return get_pi();
}
```

**Error**: `error[E0001]: expected '}', found f`

**Recommendation**: These tests should be rewritten to use top-level function definitions instead of
nested ones, OR the parser should be updated to support nested functions (if that's desired).

### 2. Void Return Types (Execution Layer Limitation)

**Status**: Partially Implemented (Codegen works, execution doesn't)

**Affected Tests**:

- `call-simple.glsl` - Test `test_call_simple_void` fails
- `return-void.glsl` - All tests fail
- `param-out.glsl` - Tests with void functions fail
- `param-inout.glsl` - Tests with void functions fail
- All tests that call void functions

**Issue**:

- Codegen correctly handles void functions (compiles them)
- Execution layer (`execute_fn.rs:152`) doesn't handle void return types
- Error: `unsupported return type: Void`

**Root Cause**: In `lp-glsl/lp-glsl-compiler/src/exec/execute_fn.rs`, the `execute_function`
function has a match statement that handles all return types except `Type::Void`. When a void
function is called, it hits the
`_ => anyhow::bail!("unsupported return type: {:?}", sig.return_type)` case.

**Code Location**: `lp-glsl/lp-glsl-compiler/src/exec/execute_fn.rs:152`

**Fix Required**: Add `Type::Void => Ok(GlslValue::F32(0.0))` or similar to handle void functions.
The test framework expects a value (typically `0.0`) when calling void functions.

### 3. Out/InOut Parameters (Not Implemented)

**Status**: Not Implemented (Parsed but not handled correctly)

**Affected Tests**:

- `param-out.glsl` - All 7 tests fail
- `param-inout.glsl` - All tests fail
- `param-mixed.glsl` - Likely fails
- `edge-const-out-error.glsl` - Edge case test
- `edge-inout-both.glsl` - Edge case test
- `edge-lvalue-out.glsl` - Edge case test
- `edge-out-not-read.glsl` - Edge case test
- `edge-out-uninitialized.glsl` - Edge case test

**Issue**:

- Parameter qualifiers (`out`, `inout`) are correctly parsed and stored in `Parameter.qualifier`
- Signature builder (`signature.rs`) ignores qualifiers and passes all parameters by value
- For `out`/`inout` parameters, they should be passed by reference (pointer)

**Current Behavior**:

- `out` parameters: Values assigned inside function don't propagate back to caller
- `inout` parameters: Values are copied in but modifications don't propagate back

**Example from `param-out.glsl`**:

```glsl
void set_value(out float result) {
    result = 42.0;
}
float test_param_out_simple() {
    float value;
    set_value(value);
    return value;  // Returns 0.0 instead of 42.0
}
```

**Root Cause**:

1. `SignatureBuilder::add_parameters()` in `signature.rs` doesn't check `param.qualifier`
2. `prepare_call_arguments()` in `function.rs` doesn't handle `out`/`inout` - it just passes values
3. Function body codegen doesn't handle writing back to `out`/`inout` parameters

**Fix Required**:

1. Update `SignatureBuilder::add_parameters()` to pass `out`/`inout` parameters as pointers
2. Update `prepare_call_arguments()` to take addresses of `out`/`inout` arguments
3. Update function body codegen to dereference `out`/`inout` parameters and write back on return

### 4. Overload Resolution (Depends on Nested Functions)

**Status**: Not Implemented (Tests use nested functions which aren't supported)

**Affected Tests**:

- `overload-resolution.glsl` - All tests fail (uses nested functions)
- `overload-ambiguous.glsl` - Likely fails
- `overload-same-name.glsl` - Likely fails

**Issue**: These tests use nested function definitions, which the parser doesn't support. Even if
overload resolution were implemented, these tests would need to be rewritten.

**Note**: The function registry (`FunctionRegistry`) appears to support multiple functions with the
same name (overloading), but the tests can't run due to parser limitations.

### 5. Forward Declarations (Needs Verification)

**Status**: Unknown (Test uses void and out parameters)

**Affected Tests**:

- `forward-declare.glsl` - All tests fail

**Issue**: Test uses:

- Void functions (`void initialize_data(out float[3] data)`)
- Out parameters (which aren't implemented)
- Array parameters with out qualifier

**Needs Investigation**: Check if forward declarations are implemented in the parser/semantic
analysis, or if failures are only due to void/out parameter issues.

### 6. Other Function Features

**Tests that likely fail due to combinations of above issues**:

- `call-multiple.glsl` - May have void function calls
- `call-nested.glsl` - Nested calls (different from nested definitions)
- `call-order.glsl` - Uses void functions (`reset_counter()`)
- `call-return-value.glsl` - May have void issues
- `declare-prototype.glsl` - Prototype declarations
- `edge-array-size-match.glsl` - Array parameter edge cases
- `edge-return-type-match.glsl` - Return type validation
- `param-array.glsl` - Array parameters
- `param-const.glsl` - Const parameters
- `param-default-in.glsl` - Default parameter qualifiers
- `param-in.glsl` - In parameters (should work, but may have void issues)
- `param-struct.glsl` - Struct parameters
- `param-unnamed.glsl` - Unnamed parameters
- `recursive-static-error.glsl` - Recursion detection
- `return-array.glsl` - Array return types
- `return-early.glsl` - Early returns
- `return-matrix.glsl` - Matrix return types
- `return-multiple.glsl` - Multiple return statements
- `return-struct.glsl` - Struct return types
- `return-vector.glsl` - Vector return types
- `scope-global.glsl` - Global scope functions
- `scope-local.glsl` - Local scope (may use nested functions)

## Implementation Priority

### High Priority (Blocks Most Tests)

1. **Void Return Types** - Quick fix, unblocks many tests
    - Add void handling to `execute_fn.rs`
    - Estimated effort: 1-2 hours

2. **Out/InOut Parameters** - Core feature, needed for psrdnoise
    - Update signature building
    - Update call argument preparation
    - Update function body codegen
    - Estimated effort: 1-2 days

### Medium Priority

3. **Nested Function Definitions** - Parser enhancement
    - Either update parser to support nested functions
    - OR rewrite tests to use top-level functions
    - Estimated effort: 2-3 days (parser) or 1-2 hours (test rewrite)

### Low Priority

4. **Overload Resolution** - Depends on nested functions
    - Verify if already implemented
    - Test with top-level functions
    - Estimated effort: 1-2 hours (verification)

5. **Forward Declarations** - Verify implementation
    - Check parser/semantic analysis
    - Fix if needed
    - Estimated effort: 2-4 hours

## Recommendations

1. **Immediate Actions**:
    - Fix void return type handling in execution layer (quick win)
    - Implement out/inout parameters (needed for psrdnoise)

2. **Test Cleanup**:
    - Rewrite tests that use nested function definitions to use top-level functions
    - This will allow proper testing of other features

3. **Verification**:
    - Test features independently to isolate issues
    - Create minimal test cases for each feature

4. **Documentation**:
    - Document which GLSL features are supported
    - Note any deviations from standard GLSL (e.g., nested functions)

## Code Locations

### Void Return Types

- Execution: `lp-glsl/lp-glsl-compiler/src/exec/execute_fn.rs:152`
- Codegen: `lp-glsl/lp-glsl-compiler/src/frontend/codegen/helpers.rs:16-18`
- Codegen: `lp-glsl/lp-glsl-compiler/src/frontend/codegen/stmt/return.rs:168-169`

### Out/InOut Parameters

- Parsing: `lp-glsl/lp-glsl-compiler/src/frontend/semantic/passes/function_signature.rs:76-95`
- Signature: `lp-glsl/lp-glsl-compiler/src/frontend/codegen/signature.rs:57-60`
- Call args: `lp-glsl/lp-glsl-compiler/src/frontend/codegen/expr/function.rs:323-383`

### Nested Functions

- Parser: GLSL parser (external crate) - doesn't support nested function definitions
