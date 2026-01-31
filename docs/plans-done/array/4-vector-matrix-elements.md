# Phase 4: Vector/Matrix Element Arrays

## Overview

Support arrays of vectors and matrices (`vec4 arr[5]`, `mat3 arr[3]`) with component access (`arr[i].x`).

## Success Criteria

Test file: `lightplayer/crates/lp-glsl-filetests/filetests/array/phases/4-vector-matrix-elements.glsl`

- Array of vectors: `vec4 arr[3];`
- Component access: `arr[0].x`, `arr[1].y`
- Array of matrices: `mat3 mats[2];`
- Matrix element access: `mats[0][0][0]`

## Implementation Tasks

### 1. Element Size Calculation

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/context.rs`

- Update `declare_variable()` to calculate element size for vectors/matrices:
  - Vector: `component_count * base_type.bytes()` (e.g., `vec4` = 4 * 4 = 16 bytes)
  - Matrix: `rows * cols * 4` (always float = 4 bytes)
  - Scalar: `element_type.bytes()` (already handled)

### 2. Component Access from Array Elements

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/lvalue.rs`

- Update `resolve_lvalue()` to handle `Expr::Dot` on `ArrayElement`:
  - Extract component indices from field name
  - Create `LValue::ArrayElement` with `component_indices` set
  - Calculate component offsets: `element_offset + component_offset`

### 3. Read Component from Array Element

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/lvalue.rs`

- In `read_lvalue()` for `ArrayElement`:
  - If `component_indices` is `Some`, load only those components
  - Calculate offsets: `element_offset + component_offset` for each component
  - Use `load` for each needed component
  - Return values matching component selection

### 4. Write Component to Array Element

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/lvalue.rs`

- In `write_lvalue()` for `ArrayElement`:
  - If `component_indices` is `Some`, store only to those components
  - Calculate offsets for each component
  - Use `store` for each component

### 5. Matrix Element Access

- Handle `arr[i][j][k]` pattern:
  - First bracket: array indexing → `ArrayElement`
  - Second bracket: matrix column indexing → `MatrixColumn`
  - Third bracket: matrix element indexing → `MatrixElement`
- Or handle as nested array access if matrix is stored as array

## Key Implementation Notes

- **Element size**: Vectors/matrices have multi-component elements
- **Component offsets**: Within element, components are at `component_index * component_size`
- **Total offset**: `element_offset + component_offset`
- **Efficiency**: Load only needed components (LValue path)

## Dependencies

- Phase 1 (Foundation) - need basic array support
- Vectors and matrices must be supported (already implemented)

## Files to Modify

- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/context.rs`
- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/lvalue.rs`





