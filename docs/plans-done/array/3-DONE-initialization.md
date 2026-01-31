# Phase 3: Initialization

## Overview

Implement array initializer lists with full and partial initialization support, and unsized arrays with size inferred from initializer.

## Success Criteria

Test file: `lightplayer/crates/lp-glsl-filetests/filetests/array/phases/3-initialization.glsl`

- Full initialization: `int arr[3] = {10, 20, 30};`
- Partial initialization: `int arr[5] = {1, 2, 3};` (remaining zeros)
- Unsized arrays: `int arr[] = {100, 200, 300};` (size inferred)

## Implementation Tasks

### 1. Handle Initializer::List

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/stmt/declaration.rs`

- Update `emit_initializer()` to handle `Initializer::List`
- Currently only handles `Initializer::Simple`
- Parse list of initializers recursively

### 2. Full Initialization

- For `int arr[3] = {10, 20, 30}`:
  - Parse list: `[Simple(10), Simple(20), Simple(30)]`
  - Evaluate each element expression
  - Validate list length matches array size
  - Store values to array elements sequentially using `store` with calculated offsets

### 3. Partial Initialization

- For `int arr[5] = {1, 2, 3}`:
  - Parse list and evaluate elements
  - Validate list length <= array size
  - Store provided values
  - Initialize remaining elements with zeros/defaults using `store`

### 4. Unsized Arrays

- Handle `ArraySpecifierDimension::Unsized` in type parsing
- When parsing declaration with unsized array:
  - Check for initializer
  - Infer size from initializer list length
  - Convert to sized array type
  - Continue with normal initialization

### 5. Multi-dimensional Initialization

- Handle nested `Initializer::List` for multi-dimensional arrays
- Recursively process nested lists
- Calculate flat offsets for multi-dimensional storage

## Key Implementation Notes

- **Initializer structure**: `Initializer::List(NonEmpty<Initializer>)` - nested lists for multi-dim
- **Zero initialization**: Use `iconst(0)` or `f32const(0.0)` for uninitialized elements
- **Type coercion**: Coerce initializer values to match element type (like existing initialization)
- **Size inference**: For unsized arrays, size = initializer list length

## Dependencies

- Phase 1 (Foundation) - need array storage and indexing

## Files to Modify

- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/stmt/declaration.rs`
- `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/type_resolver.rs` (for unsized array handling)

