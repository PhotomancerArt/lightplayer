# Phase 2: Bounds Checking

## Overview

Implement runtime bounds checking for array reads and writes using `trapnz` to prevent out-of-bounds access.

## Success Criteria

Test file: `lightplayer/crates/lp-glsl-filetests/filetests/array/phases/2-bounds-checking.glsl`

- Valid array access works
- Out-of-bounds access traps at runtime (commented out in test since it would trap)

## Implementation Tasks

### 1. Bounds Check Helper Function

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/component.rs`

- Create or extend `emit_bounds_check()` function
- Check: `index < 0 || index >= array_size`
- Use `icmp` to compare index with bounds
- Use `trapnz` with `TrapCode::user()` to trap on violation
- Pattern:
  ```rust
  let zero = builder.ins().iconst(types::I32, 0);
  let index_lt_zero = builder.ins().icmp(IntCC::SignedLessThan, index, zero);
  let array_size = builder.ins().iconst(types::I32, array_size as i64);
  let index_ge_size = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, index, array_size);
  let out_of_bounds = builder.ins().bor(index_lt_zero, index_ge_size);
  let trap_code = TrapCode::user(1).unwrap();
  builder.ins().trapnz(out_of_bounds, trap_code);
  ```

### 2. Add Bounds Checks to Array Reads

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/lvalue.rs`

- In `read_lvalue()` for `ArrayElement`:
  - Generate bounds check before `load`
  - Use runtime index value for bounds check

### 3. Add Bounds Checks to Array Writes

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/lvalue.rs`

- In `write_lvalue()` for `ArrayElement`:
  - Generate bounds check before `store`
  - Use runtime index value for bounds check

### 4. Optimize Compile-Time Constant Indices

- Skip bounds check for compile-time constant indices (already validated at compile-time)
- Only generate bounds checks for runtime indices

## Key Implementation Notes

- **Always check for writes**: Required for safety
- **Check for reads by default**: Can add feature flag later if performance is critical
- **Compile-time constants**: Skip bounds check (already validated)
- **Trap code**: Use `TrapCode::user(1)` for "array out of bounds"

## Dependencies

- Phase 1 (Foundation) - need array indexing working first

## Files to Modify

- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/component.rs`
- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/lvalue.rs`

