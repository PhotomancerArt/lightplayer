# Phase 7: Function Parameters

## Overview

Support arrays as function parameters and return values, passing by pointer.

## Success Criteria

Test file: `lightplayer/crates/lp-glsl-filetests/filetests/array/phases/7-function-parameters.glsl`

- Array as function parameter: `int sum_array(int arr[5])`
- Pass array to function: `sum_array(arr)`
- Array as return value (if supported)

## Implementation Tasks

### 1. Function Signature with Arrays

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/signature.rs`

- Update signature building to handle array types
- Arrays passed as pointers (not by value)
- Use `pointer_type` for array parameters

### 2. Parameter Parsing

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/passes/function_signature.rs`

- Update parameter type parsing to handle arrays
- Use unified `parse_type_specifier()` (from Phase 1)

### 3. Function Call with Array Arguments

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/function.rs`

- When calling function with array argument:
  - Get array pointer from `VarInfo`
  - Pass pointer as argument (not array contents)
  - Use pointer type in function signature

### 4. Array Return Values

- If GLSL supports array return values:
  - Use struct return pattern (like matrices)
  - Return pointer to array
- Or verify GLSL spec on array returns

### 5. Bounds Checking in Functions

- Array parameters need bounds information
- May need to pass array size as additional parameter
- Or use array length method if available

## Key Implementation Notes

- **Pass by pointer**: Arrays are large, pass pointer not values
- **Pointer type**: Use `isa.pointer_type()` for array parameters
- **Array size**: May need to track array size for bounds checking in functions
- **GLSL spec**: Verify array return value support

## Dependencies

- Phase 1 (Foundation) - need array storage working
- Phase 2 (Bounds Checking) - for bounds checks in functions

## Files to Modify

- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/signature.rs`
- `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/passes/function_signature.rs`
- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/function.rs`





