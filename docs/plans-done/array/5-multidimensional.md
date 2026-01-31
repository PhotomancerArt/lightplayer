# Phase 5: Multi-dimensional Arrays

## Overview

Support nested arrays (`float[5][3]`) with multi-dimensional indexing (`arr[i][j]`).

## Success Criteria

Test file: `lightplayer/crates/lp-glsl-filetests/filetests/array/phases/5-multidimensional.glsl`

- 2D array declaration: `int arr[3][2];`
- Multi-dimensional indexing: `arr[0][0]`, `arr[1][1]`, `arr[2][0]`

## Implementation Tasks

### 1. Multi-dimensional Type Parsing

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/type_resolver.rs`

- Already handled in Phase 1 (recursive parsing)
- Verify it works for nested arrays: `float[5][3]` → `Array(Box<Array(Box<Float>, 3)>, 5)`

### 2. Multi-dimensional Storage

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/context.rs`

- Calculate total size for multi-dimensional arrays:
  - For `float[5][3]`: total = 5 * 3 * 4 = 60 bytes
  - Use `array_total_element_count()` recursively
- Allocate single stack slot for entire array (flat storage)

### 3. Multi-dimensional Indexing

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/component.rs`

- Handle multiple dimensions in `ArraySpecifier`
- Calculate flat offset: `offset = (i0 * size1 + i1) * element_size_bytes` for 2D
- Generalize for N dimensions: `offset = (i0 * size1 * size2 * ... + i1 * size2 * ... + i2 * ...) * element_size_bytes`
- Process dimensions from outermost to innermost (matches type structure)

### 4. Multi-dimensional LValue Resolution

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/lvalue.rs`

- Update `resolve_lvalue()` to handle multiple dimensions:
  - Process each dimension in `ArraySpecifier`
  - Calculate cumulative offset
  - Return `ArrayElement` with final offset

### 5. Bounds Checking for Multi-dimensional

- Check bounds for each dimension
- Generate bounds checks: `index0 < size0 && index1 < size1 && ...`

## Key Implementation Notes

- **Storage**: Flat storage (single stack slot), not nested allocations
- **Offset calculation**: `offset = (i0 * size1 + i1) * element_size_bytes` for 2D
- **Dimension order**: Outermost-first (matches type structure from Phase 1)
- **Bounds checking**: Check each dimension separately

## Dependencies

- Phase 1 (Foundation) - need basic array support
- Phase 2 (Bounds Checking) - for multi-dimensional bounds checks

## Files to Modify

- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/component.rs`
- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/lvalue.rs`
- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/context.rs` (verify multi-dim size calculation)





