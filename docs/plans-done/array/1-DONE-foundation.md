# Phase 1: Foundation

## Overview

Implement basic 1D scalar arrays with literal integer sizes, using stack allocation and pointer-based storage. Support basic read/write access.

## Success Criteria

Test file: `lightplayer/crates/lp-glsl-filetests/filetests/array/phases/1-foundation.glsl`

- Array declaration: `int arr[5];`
- Array write: `arr[0] = 10;`
- Array read: `int x = arr[0];`

## Implementation Tasks

### 1. Type System Extensions

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/types.rs`

- Add `is_array() -> bool`
- Add `array_element_type() -> Option<Type>` (recursive for multi-dim)
- Add `array_dimensions() -> Vec<usize>` (outermost-first)
- Add `array_total_element_count() -> Option<usize>` (for variable allocation)
- Update `to_cranelift_type()` to return element type for arrays

### 2. Type Parsing

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/type_resolver.rs`

- Update `parse_type_specifier()` to check `type_spec.ty.array_specifier`
- Parse `ArraySpecifier` dimensions recursively (outermost-first)
- Extract literal integer sizes from `ExplicitlySized(Expr::IntConst)`
- Build nested `Array(Box<Type>, usize)` types
- Update `declaration.rs::parse_type_specifier()` to call unified function

### 3. Variable Declaration with Stack Allocation

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/context.rs`

- Update `VarInfo` to support arrays:
  ```rust
  array_ptr: Option<Value>,
  stack_slot: Option<StackSlot>,
  ```
- Update `declare_variable()` to:
  - Detect `Type::Array`
  - Calculate total size: `array_size * element_size_bytes`
  - Allocate stack slot: `create_sized_stack_slot()`
  - Get pointer: `stack_addr()`
  - Store pointer in `VarInfo` (not `Vec<Variable>`)

### 4. Array Indexing (RValue Path)

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/component.rs`

- Extend `emit_indexing()` to handle arrays before matrix/vector check
- For arrays:
  - Look up array pointer from `VarInfo`
  - Evaluate index expression
  - Calculate byte offset: `offset = index * element_size_bytes`
  - Use `load` with pointer + offset
  - Return element type and value(s)

### 5. Array Indexing (LValue Path)

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/lvalue.rs`

- Add `ArrayElement` variant to `LValue` enum:
  ```rust
  ArrayElement {
      array_ptr: Value,
      base_ty: GlslType,
      index: Option<usize>,      // Compile-time
      index_val: Option<Value>,  // Runtime
      element_ty: GlslType,
      element_size_bytes: usize,
      component_indices: Option<Vec<usize>>,
  }
  ```
- Update `resolve_lvalue()` to handle `Expr::Bracket` for arrays
- Update `read_lvalue()` to handle `ArrayElement` (use `load` with pointer + offset)
- Update `write_lvalue()` to handle `ArrayElement` (use `store` with pointer + offset)
- Update `LValue::ty()` to return element type for `ArrayElement`

### 6. Type Inference

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/type_check/inference.rs`

- Update `infer_expr_type_with_registry()` for `Expr::Bracket`
- Check if base type is array before matrix/vector
- Return element type (recursive for multi-dim)
- Validate index type is `Int`

## Key Implementation Notes

- **Storage**: Stack-allocated memory blocks, not individual variables
- **Element size**: For scalars, use `element_type.bytes()` (e.g., `I32` = 4 bytes)
- **Offset calculation**: `offset = index * element_size_bytes`
- **Compile-time vs runtime**: Support both constant and variable indices
- **Component access**: Defer to Phase 4 (vector/matrix element arrays)

## Dependencies

- None (foundation phase)

## Files to Modify

- `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/types.rs`
- `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/type_resolver.rs`
- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/stmt/declaration.rs`
- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/context.rs`
- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/component.rs`
- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/lvalue.rs`
- `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/type_check/inference.rs`

