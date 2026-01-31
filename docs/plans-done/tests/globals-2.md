# Plan: Document Existing Global Variable Tests

## Overview

This plan documents the existing global variable tests in `lightplayer/crates/lp-glsl-filetests/filetests/globals/`. These tests comprehensively cover GLSL global variables including declarations, storage qualifiers, initialization, scope, access from functions, and edge cases.

## Existing Test Structure

The global tests are organized in a flat structure with 45 test files:

```
globals/
├── access-from-function.glsl
├── access-from-main.glsl
├── access-read.glsl
├── access-write.glsl
├── const-expression.glsl
├── const-must-init.glsl
├── const-readonly.glsl
├── declare-buffer.glsl
├── declare-const.glsl
├── declare-in.glsl
├── declare-out.glsl
├── declare-shared.glsl
├── declare-simple.glsl
├── declare-uniform.glsl
├── edge-const-write-error.glsl
├── edge-empty-declaration.glsl
├── edge-in-write-error.glsl
├── edge-multiple-qualifiers-error.glsl
├── edge-no-storage-qualifier.glsl
├── edge-out-read-error.glsl
├── edge-uninitialized-read.glsl
├── edge-uniform-write-error.glsl
├── forward-reference.glsl
├── initialize-const.glsl
├── initialize-simple.glsl
├── initialize-uniform.glsl
├── initialize-undefined.glsl
├── multiple-declare.glsl
├── out-writeonly.glsl
├── redeclare-error.glsl
├── scope-global.glsl
├── scope-nested.glsl
├── scope-shadowing.glsl
├── shared-array-size.glsl
├── shared-globals.glsl
├── shared-multiple-init.glsl
├── shared-struct-match.glsl
├── type-array.glsl
├── type-matrix.glsl
├── type-scalar.glsl
├── type-struct.glsl
├── type-vector.glsl
├── uniform-no-init-error.glsl
└── uniform-readonly.glsl
```

## Test Coverage

### Global Variable Declarations

**declare-simple.glsl**: Simple global declarations (no storage qualifier)
- Unqualified globals
- Default storage qualifier
- Local memory

**declare-const.glsl**: Const global declarations
- `const` qualifier
- Read-only variables
- Must be initialized

**declare-uniform.glsl**: Uniform global declarations
- `uniform` qualifier
- Application linkage
- Read-only in shader

**declare-in.glsl**: In global declarations
- `in` qualifier
- Input from previous stage
- Read-only

**declare-out.glsl**: Out global declarations
- `out` qualifier
- Output to next stage
- Write-only (or read-write)

**declare-buffer.glsl**: Buffer global declarations
- `buffer` qualifier
- Shader storage buffer
- Read-write

**declare-shared.glsl**: Shared global declarations
- `shared` qualifier
- Compute shader only
- Shared across workgroup

### Global Variable Initialization

**initialize-simple.glsl**: Simple global initialization
- Initialized globals
- Uninitialized globals

**initialize-const.glsl**: Const global initialization
- Const with constant expressions
- Must be initialized

**initialize-uniform.glsl**: Uniform initialization
- Uniform initialization rules
- If allowed

**initialize-undefined.glsl**: Uninitialized globals
- Undefined values
- Enter main() uninitialized

### Global Variable Access

**access-read.glsl**: Reading global variables
- Read from globals
- Various types

**access-write.glsl**: Writing to global variables
- Write to globals
- Various types

**access-from-function.glsl**: Accessing globals from functions
- Functions can read/write globals
- Global access

**access-from-main.glsl**: Accessing globals from main
- Main can access globals
- Entry point access

### Scope Rules

**scope-global.glsl**: Global scope
- Globals visible everywhere
- Global scope rules

**scope-shadowing.glsl**: Local variables shadowing globals
- Shadowing behavior
- Hidden globals

**scope-nested.glsl**: Nested scopes and globals
- Globals in nested scopes
- Scope boundaries

### Global Variable Types

**type-scalar.glsl**: Scalar global types
- float, int, uint, bool

**type-vector.glsl**: Vector global types
- vec2, vec3, vec4
- ivec, uvec, bvec

**type-matrix.glsl**: Matrix global types
- mat2, mat3, mat4

**type-array.glsl**: Array global types
- Array globals
- Array sizes

**type-struct.glsl**: Struct global types
- Struct globals
- Struct initialization

### Const Qualifier

**const-readonly.glsl**: Const variables are read-only
- Cannot modify const
- Read-only restriction

**const-must-init.glsl**: Const variables must be initialized
- Initialization required
- Compile error if not initialized

**const-expression.glsl**: Const initializers must be constant expressions
- Constant expression requirement
- Non-constant expression errors

### Storage Qualifier Restrictions

**uniform-readonly.glsl**: Uniform variables are read-only
- Cannot modify uniform
- Read-only restriction

**uniform-no-init-error.glsl**: Uniform initialization restrictions
- Uniform initialization rules
- If not allowed

**in-readonly.glsl**: In variables are read-only (if exists)
- Cannot modify in
- Read-only restriction

**out-writeonly.glsl**: Out variables are write-only
- Write-only behavior
- Read restrictions

### Multiple Declarations and Redeclarations

**multiple-declare.glsl**: Multiple global declarations
- Multiple globals
- Different types

**redeclare-error.glsl**: Redeclaration errors
- Same name in same scope - error
- Different scope - allowed

**forward-reference.glsl**: Forward reference to globals
- Global declared after use
- Forward reference rules

### Shared Globals

**shared-globals.glsl**: Shared globals
- Same name across shaders
- Same type requirement
- Shared storage

**shared-array-size.glsl**: Shared global arrays
- Same base type and size
- Array size matching

**shared-struct-match.glsl**: Shared struct globals
- Same struct definition
- Member matching

**shared-multiple-init.glsl**: Shared globals with multiple initializers
- Multiple initializers
- Must be same value
- Constant expressions

### Edge Cases

**edge-uninitialized-read.glsl**: Reading uninitialized globals
- Undefined values
- Undefined behavior

**edge-const-write-error.glsl**: Writing to const - compile error
- Const cannot be written
- Compile error

**edge-uniform-write-error.glsl**: Writing to uniform - compile error
- Uniform cannot be written
- Compile error

**edge-in-write-error.glsl**: Writing to in - compile error
- Input cannot be modified
- Compile error

**edge-out-read-error.glsl**: Reading from out - compile error (if applicable)
- Output read restrictions
- May be error depending on stage

**edge-no-storage-qualifier.glsl**: Default storage qualifier
- No qualifier behavior
- Default qualifier

**edge-multiple-qualifiers-error.glsl**: Multiple storage qualifiers - compile error
- Only one qualifier allowed
- Compile error

**edge-empty-declaration.glsl**: Empty declarations
- Empty declarations
- If applicable

## GLSL Spec References

- **variables.adoc**: Storage Qualifiers (lines 1834-1933), Scoping (lines 1403-1831), Constant Qualifier (lines 1936-1978)
- Key sections:
  - Global scope rules
  - Storage qualifiers (const, uniform, in, out, buffer, shared)
  - Initialization rules (constant expressions, undefined values)
  - Shared globals (for GLSL)
  - Read-only restrictions
  - Scope and shadowing rules






