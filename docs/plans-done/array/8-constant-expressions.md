# Phase 8: Constant Expression Array Sizes

## Overview

Support constant expressions for array sizes beyond literal integers (`const int n = 5; float arr[n]`, `float arr[5+3]`).

## Success Criteria

Test file: `lightplayer/crates/lp-glsl-filetests/filetests/array/phases/8-constant-expressions.glsl`

- Constant variable: `const int n = 5; int arr[n];`
- Constant expression: `int arr[3 + 2];`
- Multiple constants: `const int a = 2; const int b = 3; int arr[a * b];`

## Implementation Tasks

### 1. Constant Expression Evaluator

**File**: New file or `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/constant_eval.rs`

- Create constant expression evaluator
- Evaluate compile-time constant expressions:
  - Literal integers
  - `const` variables
  - Arithmetic: `+`, `-`, `*`, `/`, `%`
  - Parentheses
- Return `Option<i32>` (None if not constant)

### 2. Update Array Size Parsing

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/type_resolver.rs`

- Update `parse_type_specifier()` to evaluate constant expressions
- For `ExplicitlySized(expr)`:
  - Try constant evaluation
  - If constant: use value
  - If not constant: error (GLSL requires compile-time constant)

### 3. Handle const Variables

- Look up `const` variables in symbol table
- Evaluate their initializers as constant expressions
- Use in array size calculations

### 4. Support Constant Expressions

- Evaluate expressions like `5 + 3`, `2 * 4`, `(5 + 3) * 2`
- Support all arithmetic operators
- Validate result is positive integer

## Key Implementation Notes

- **Constant only**: GLSL requires compile-time constant array sizes
- **Evaluation order**: Evaluate from innermost to outermost
- **Error handling**: Non-constant expressions should error
- **const variables**: Must be initialized with constant expressions

## Dependencies

- Phase 1 (Foundation) - need basic array support
- Symbol table with const variable support

## Files to Modify/Create

- New: `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/constant_eval.rs` (or similar)
- `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/type_resolver.rs`
- `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/type_check/inference.rs` (may need const variable lookup)





