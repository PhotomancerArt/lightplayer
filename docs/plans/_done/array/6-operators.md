# Phase 6: Verify All Operators

## Overview

Verify that increment/decrement, compound assignment, and binary/unary operations work correctly on array elements. These should work automatically via the LValue pattern, but need verification and testing.

## Success Criteria

Test file: `lightplayer/crates/lp-glsl-filetests/filetests/array/phases/6-operators.glsl`

- Increment/decrement: `arr[i]++`, `++arr[i]`, `arr[i]--`, `--arr[i]`
- Compound assignment: `arr[i] += x`, `arr[i] -= x`, `arr[i] *= x`, `arr[i] /= x`
- Binary operations: `arr[i] + x`, `arr[i] - x`, `arr[i] * x`
- Unary operations: `-arr[i]`, `+arr[i]`

## Implementation Tasks

### 1. Verify Increment/Decrement

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/incdec.rs`

- Should work automatically via LValue pattern
- Verify `ArrayElement` works with `read_lvalue()` and `write_lvalue()`
- Test pre-increment, post-increment, pre-decrement, post-decrement

### 2. Verify Compound Assignment

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/assignment.rs`

- Should work automatically via LValue pattern
- Verify `ArrayElement` works with compound assignment operators
- Test `+=`, `-=`, `*=`, `/=`
- Check if `%=` is needed (verify GLSL spec)

### 3. Verify Binary Operations

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/binary.rs`

- Should work automatically - array elements as RValues
- Verify `read_lvalue()` returns values that work in binary operations
- Test arithmetic: `+`, `-`, `*`, `/`, `%`
- Test comparison: `==`, `!=`, `<`, `>`, `<=`, `>=`
- Test logical: `&&`, `||`, `^^`

### 4. Verify Unary Operations

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/unary.rs`

- Should work automatically - array elements as RValues
- Test unary: `+`, `-`, `!`

### 5. Fix Any Issues Found

- If operators don't work automatically, identify and fix the issues
- May need to ensure `read_lvalue()` returns proper RValue format
- May need type checking updates

## Key Implementation Notes

- **Should be automatic**: LValue pattern should make this work
- **Verification phase**: Mainly testing and fixing edge cases
- **Type checking**: Ensure array elements work in all operator contexts

## Dependencies

- Phase 1 (Foundation) - need `ArrayElement` LValue working
- Phase 2 (Bounds Checking) - bounds checks should work with operators

## Files to Verify/Modify

- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/incdec.rs` (verify)
- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/assignment.rs` (verify)
- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/binary.rs` (verify)
- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/unary.rs` (verify)
- `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/type_check/operators.rs` (may need updates)





