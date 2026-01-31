# Phase 4: Add Lvalue Validation

## Description

Add compile-time validation that out/inout arguments are lvalues (variables, array elements, etc.),
not expressions.

## Implementation

### Files to Modify

1. **`lp-glsl/lp-glsl-compiler/src/frontend/codegen/expr/function.rs`**
    - Add `validate_out_inout_arguments()` function
    - Call validation before preparing call arguments

### Changes

1. **`validate_out_inout_arguments()`** (NEW)
    - For each out/inout parameter:
        - Try to resolve argument expression as lvalue using `resolve_lvalue()`
        - If `resolve_lvalue()` fails: Emit compile-time error
        - Error message: "expression is not a valid lvalue for out/inout parameter"

2. **`emit_user_function_call()`**
    - Call `validate_out_inout_arguments()` before `prepare_call_arguments()`
    - Validation happens in semantic checking phase (before codegen)

### Success Criteria

- Non-lvalue arguments to out/inout parameters produce compile-time errors
- Valid lvalue arguments (variables, array elements, vector components, struct fields) pass
  validation
- Error messages are clear and point to the problematic argument
- Code compiles without errors

## Notes

- Reuse existing `resolve_lvalue()` function for validation
- Valid lvalues: variables, array elements, vector components (swizzles), struct fields, matrix
  elements/columns
- Invalid: literals, function call results, expressions like `x + y`
- Validation happens early (before codegen) for better error messages
