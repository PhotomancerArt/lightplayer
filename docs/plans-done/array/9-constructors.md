# Phase 9: Array Constructors

## Overview

Support array constructor syntax (`float[5](1.0, 2.0, 3.0, 4.0, 5.0)`).

## Success Criteria

Test file: `lightplayer/crates/lp-glsl-filetests/filetests/array/phases/9-constructors.glsl`

- Explicit size constructor: `int arr1 = int[3](10, 20, 30);`
- Inferred size constructor: `int arr2 = int[](1, 2, 3, 4, 5);`
- Vector array constructor: `vec4 arr3 = vec4[2](vec4(1.0), vec4(2.0));`

## Implementation Tasks

### 1. Identify Constructor Syntax

- Check how GLSL parser represents array constructors
- May be `Expr::FunCall` with array type name
- Or special constructor expression type

### 2. Parse Array Constructor

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/constructor.rs`

- Detect array constructor syntax
- Extract array type and size (if explicit)
- Extract constructor arguments
- Validate argument count matches size (if explicit)

### 3. Generate Constructor Code

- Allocate stack slot for array
- Get pointer
- Evaluate each argument expression
- Store values to array elements sequentially
- Return array pointer or values

### 4. Inferred Size Constructors

- For `int[](1, 2, 3)`:
  - Infer size from argument count
  - Allocate array of inferred size
  - Initialize with arguments

### 5. Type Checking

- Validate argument types match element type
- Support type coercion (like vector constructors)
- Check argument count matches size (if explicit)

## Key Implementation Notes

- **Syntax**: Need to verify how GLSL parser represents this
- **Storage**: Allocate temporary array on stack
- **Lifetime**: Constructor result may need special handling
- **Type coercion**: Like vector constructors, support implicit conversions

## Dependencies

- Phase 1 (Foundation) - need array storage
- Phase 3 (Initialization) - similar to initializer lists
- Phase 8 (Constant Expressions) - for explicit sizes with constants

## Files to Modify

- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/constructor.rs`
- `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/type_check/constructors.rs` (may need array constructor checking)





