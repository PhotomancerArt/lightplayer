# Phase 2: Update Function Call Codegen

## Description

Update function call codegen to handle out/inout arguments by getting addresses of lvalues and
copying values back after the call.

## Implementation

### Files to Modify

1. **`lp-glsl/lp-glsl-compiler/src/frontend/codegen/expr/function.rs`**
    - Update `prepare_call_arguments()` to handle out/inout
    - Add `copy_back_out_parameters()` function
    - Update `emit_user_function_call()` to call copy-back

### Changes

1. **`prepare_call_arguments()`**
    - For out/inout parameters: Resolve argument as lvalue using `resolve_lvalue()`
    - Get address of lvalue (use `get_lvalue_address()` helper)
    - Pass pointer as argument
    - Track which arguments are out/inout for later copy-back
    - For in parameters: Continue existing behavior

2. **`copy_back_out_parameters()`** (NEW)
    - After function call completes
    - For each out/inout parameter:
        - Load values from pointer (handle vectors/matrices)
        - Store values back to original lvalue
    - Copy back in parameter order (left to right)

3. **`emit_user_function_call()`**
    - After `execute_function_call()`, call `copy_back_out_parameters()`
    - Handle return values as before

### Helper Functions Needed

- `get_lvalue_address()`: Get pointer to lvalue (for variables, array elements, etc.)
- `store_to_lvalue()`: Store values back to lvalue (handle vectors/matrices)

### Success Criteria

- Function calls with out/inout parameters compile
- Addresses are correctly passed for out/inout arguments
- Values are copied back after call completes
- Existing in-parameter calls continue to work
- Code compiles without errors

## Notes

- Copy-back happens immediately after call (not deferred)
- Copy-back order: left to right (parameter order)
- For vectors/matrices: Load all components from pointer, store to lvalue
- Handle StructReturn parameter (if present) before out parameters
