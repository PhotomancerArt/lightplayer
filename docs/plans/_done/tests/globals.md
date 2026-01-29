# Plan: Create Comprehensive Global Variable Tests

## Overview

Create a complete test suite for GLSL global variables in `lightplayer/crates/lp-glsl-filetests/filetests/globals/` following the flat naming convention with prefixes. These tests will comprehensively cover the GLSL global variable specification including declarations, storage qualifiers (`const`, `uniform`, `in`, `out`, `buffer`, `shared`), initialization, scope, access from functions, and edge cases. These tests are expected to fail initially, serving as a specification for implementing global variable support in the compiler.

## Directory Structure

Following the flat naming convention with prefixes, create tests in a single `globals/` directory:

```javascript
globals/
├── declare-simple.glsl              (simple global declarations - no qualifier)
├── declare-const.glsl               (const global variables)
├── declare-uniform.glsl             (uniform global variables)
├── declare-in.glsl                  (in global variables - input from previous stage)
├── declare-out.glsl                 (out global variables - output to next stage)
├── declare-buffer.glsl              (buffer global variables - shader storage buffer)
├── declare-shared.glsl              (shared global variables - compute shader)
├── initialize-simple.glsl            (global initialization - no qualifier)
├── initialize-const.glsl            (const global initialization - constant expression)
├── initialize-uniform.glsl          (uniform initialization - if allowed)
├── initialize-undefined.glsl        (uninitialized globals - undefined values)
├── access-read.glsl                 (reading global variables)
├── access-write.glsl                (writing to global variables)
├── access-from-function.glsl        (accessing globals from functions)
├── access-from-main.glsl            (accessing globals from main)
├── scope-global.glsl                (global scope - visible everywhere)
├── scope-shadowing.glsl             (local variables shadowing globals)
├── scope-nested.glsl                (nested scopes and globals)
├── type-scalar.glsl                 (scalar global types: float, int, uint, bool)
├── type-vector.glsl                 (vector global types: vec2, vec3, vec4, etc.)
├── type-matrix.glsl                 (matrix global types: mat2, mat3, mat4)
├── type-array.glsl                  (array global types)
├── type-struct.glsl                 (struct global types)
├── const-readonly.glsl               (const variables are read-only)
├── const-must-init.glsl              (const variables must be initialized)
├── const-expression.glsl             (const initializers must be constant expressions)
├── uniform-readonly.glsl             (uniform variables are read-only)
├── uniform-no-init-error.glsl        (uniform cannot be initialized - if applicable)
├── in-readonly.glsl                  (in variables are read-only)
├── out-writeonly.glsl                (out variables are write-only)
├── multiple-declare.glsl             (multiple global declarations)
├── redeclare-error.glsl              (redeclaration errors in same scope)
├── forward-reference.glsl            (forward reference to globals)
├── shared-globals.glsl               (shared globals - same name, same type)
├── shared-array-size.glsl            (shared global arrays - same size)
├── shared-struct-match.glsl          (shared structs - same definition)
├── shared-multiple-init.glsl          (shared globals with multiple initializers)
├── edge-uninitialized-read.glsl       (reading uninitialized globals - undefined)
├── edge-const-write-error.glsl       (writing to const - compile error)
├── edge-uniform-write-error.glsl     (writing to uniform - compile error)
├── edge-in-write-error.glsl           (writing to in - compile error)
├── edge-out-read-error.glsl           (reading from out - compile error if applicable)
├── edge-no-storage-qualifier.glsl     (default storage qualifier behavior)
├── edge-multiple-qualifiers-error.glsl (multiple storage qualifiers - compile error)
└── edge-empty-declaration.glsl        (empty declarations - if applicable)
```

## Test File Patterns

Each test file should follow the pattern from other test suites:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

float global_var = 1.0;

float test_global_operation_name() {
    // Test implementation using global_var
    return global_var * 2.0;
    // Should be 2.0
}

// run: test_global_operation_name() ~= 2.0
```

## Key Test Categories

### 1. Global Variable Declarations

**declare-simple.glsl**: Test simple global declarations (no storage qualifier)
- `float global_var;` - unqualified global variable
- `int global_int;` - unqualified global integer
- `vec4 global_vec;` - unqualified global vector
- Default storage qualifier (no linkage, local memory)
- Variables accessible from all functions

**declare-const.glsl**: Test const global declarations
- `const float PI = 3.14159;` - const global with initializer
- `const vec3 UP = vec3(0.0, 1.0, 0.0);` - const global vector
- Const variables are read-only
- Const variables must be initialized

**declare-uniform.glsl**: Test uniform global declarations
- `uniform float time;` - uniform global
- `uniform mat4 modelView;` - uniform matrix
- Uniforms form linkage between shader and application
- Uniforms are read-only in shader

**declare-in.glsl**: Test in global declarations
- `in vec3 position;` - input from previous stage
- `in float temperature;` - input scalar
- Input variables are copied in from previous stage
- Input variables are read-only in shader

**declare-out.glsl**: Test out global declarations
- `out vec4 color;` - output to next stage
- `out float depth;` - output scalar
- Output variables are copied out to next stage
- Output variables are write-only (or read-write depending on stage)

**declare-buffer.glsl**: Test buffer global declarations
- `buffer DataBlock { float data[]; };` - shader storage buffer
- Buffer variables can be read and written
- Buffer variables shared between shader and application

**declare-shared.glsl**: Test shared global declarations
- `shared float workgroup_data;` - shared variable (compute shader)
- Shared variables shared across workgroup
- Shared variables can be read and written

### 2. Global Variable Initialization

**initialize-simple.glsl**: Test simple global initialization
- `float global_var = 5.0;` - initialized global
- `vec2 global_vec = vec2(1.0, 2.0);` - initialized vector
- Initializers for unqualified globals (if allowed)
- Uninitialized globals have undefined values

**initialize-const.glsl**: Test const global initialization
- `const float PI = 3.14159;` - const with constant expression
- `const vec3 UP = vec3(0.0, 1.0, 0.0);` - const with constructor
- Const initializers must be constant expressions
- Const variables must be initialized

**initialize-uniform.glsl**: Test uniform initialization
- Uniform initialization rules (if allowed in GLSL)
- Uniform initializers must be constant expressions (if allowed)
- Test cases where uniform initialization is not allowed

**initialize-undefined.glsl**: Test uninitialized globals
- `float global_var;` - uninitialized global
- Uninitialized globals enter main() with undefined values
- Reading uninitialized globals produces undefined behavior

### 3. Global Variable Access

**access-read.glsl**: Test reading global variables
- `float x = global_var;` - read global
- `vec4 v = global_vec;` - read global vector
- Reading from various global types
- Reading from const, uniform, in globals

**access-write.glsl**: Test writing to global variables
- `global_var = 10.0;` - write to global
- `global_vec = vec4(1.0);` - write to global vector
- Writing to unqualified globals
- Writing to out, buffer, shared globals

**access-from-function.glsl**: Test accessing globals from functions
- Functions can read global variables
- Functions can write to global variables (if allowed)
- Global access from nested function calls
- Global access from multiple functions

**access-from-main.glsl**: Test accessing globals from main
- Reading globals in main()
- Writing to globals in main()
- Globals accessible from main entry point

### 4. Scope Rules

**scope-global.glsl**: Test global scope
- Globals declared outside all functions have global scope
- Global scope persists to end of shader
- Globals visible from all functions
- Globals visible from nested scopes

**scope-shadowing.glsl**: Test local variables shadowing globals
- `float global_var = 1.0;` then `float global_var = 2.0;` in function
- Local variable hides global with same name
- No way to access hidden global from nested scope
- Test shadowing in nested scopes

**scope-nested.glsl**: Test nested scopes and globals
- Globals accessible from nested scopes
- Local variables in nested scopes can shadow globals
- Globals remain accessible after exiting nested scope

### 5. Global Variable Types

**type-scalar.glsl**: Test scalar global types
- `float global_float;` - float global
- `int global_int;` - int global
- `uint global_uint;` - uint global
- `bool global_bool;` - bool global

**type-vector.glsl**: Test vector global types
- `vec2 global_vec2;` - vec2 global
- `vec3 global_vec3;` - vec3 global
- `vec4 global_vec4;` - vec4 global
- Test with ivec2, ivec3, ivec4, uvec2, uvec3, uvec4, bvec2, bvec3, bvec4

**type-matrix.glsl**: Test matrix global types
- `mat2 global_mat2;` - mat2 global
- `mat3 global_mat3;` - mat3 global
- `mat4 global_mat4;` - mat4 global
- Test with various matrix types

**type-array.glsl**: Test array global types
- `float global_array[5];` - array global
- `vec4 global_vec_array[3];` - array of vectors
- Array globals with explicit sizes
- Array globals with unsized arrays (if allowed)

**type-struct.glsl**: Test struct global types
- `struct S { float x; float y; };`
- `S global_struct;` - struct global
- Struct globals with initialization
- Nested struct globals

### 6. Const Qualifier

**const-readonly.glsl**: Test const variables are read-only
- `const float PI = 3.14159;`
- `PI = 4.0;` - compile error
- Const variables cannot be modified after initialization
- Test with various const types

**const-must-init.glsl**: Test const variables must be initialized
- `const float PI;` - compile error (must initialize)
- `const vec3 UP;` - compile error (must initialize)
- Const variables require initializers

**const-expression.glsl**: Test const initializers must be constant expressions
- `const float x = 5.0;` - constant expression
- `const float y = 2.0 + 3.0;` - constant expression
- `const float z = non_const_var;` - compile error (not constant expression)
- Test various constant expression forms

### 7. Storage Qualifier Restrictions

**uniform-readonly.glsl**: Test uniform variables are read-only
- `uniform float time;`
- `time = 10.0;` - compile error (if applicable)
- Uniforms cannot be modified in shader

**uniform-no-init-error.glsl**: Test uniform initialization restrictions
- Uniform initialization rules (if not allowed)
- Test cases where uniform initialization causes errors

**in-readonly.glsl**: Test in variables are read-only
- `in vec3 position;`
- `position = vec3(0.0);` - compile error
- Input variables cannot be modified

**out-writeonly.glsl**: Test out variables are write-only (or read-write)
- `out vec4 color;`
- `color = vec4(1.0);` - write allowed
- Reading from out (if restricted)
- Output variables behavior depends on shader stage

### 8. Multiple Declarations and Redeclarations

**multiple-declare.glsl**: Test multiple global declarations
- Multiple globals of different types
- Multiple globals of same type with different names
- Globals declared before and after functions

**redeclare-error.glsl**: Test redeclaration errors
- `float x; float x;` - compile error (same scope)
- `float x; { float x; }` - allowed (different scope)
- Redeclaration in same scope is error

**forward-reference.glsl**: Test forward reference to globals
- Global declared after function that uses it
- Forward reference rules for globals
- Globals must be declared before use (or forward declared)

### 9. Shared Globals (GLSL)

**shared-globals.glsl**: Test shared globals
- Same global name declared in multiple shaders
- Shared globals must have same type
- Shared globals share same storage
- Test with uniforms (only shared globals in ESSL)

**shared-array-size.glsl**: Test shared global arrays
- Shared arrays must have same base type and size
- Implicitly sized arrays can be explicitly sized in another shader
- Array size matching rules

**shared-struct-match.glsl**: Test shared struct globals
- Shared structs must have same name, member types, member names
- Struct matching rules for shared globals
- Nested struct matching

**shared-multiple-init.glsl**: Test shared globals with multiple initializers
- Shared globals with initializers in multiple shaders
- All initializers must be constant expressions with same value
- Link-time error if initializers differ

### 10. Edge Cases

**edge-uninitialized-read.glsl**: Test reading uninitialized globals
- `float global_var;` - uninitialized
- `float x = global_var;` - undefined value
- Uninitialized globals have undefined values at start of main()

**edge-const-write-error.glsl**: Test writing to const - compile error
- `const float PI = 3.14159;`
- `PI = 4.0;` - compile error
- Const variables cannot be written to

**edge-uniform-write-error.glsl**: Test writing to uniform - compile error
- `uniform float time;`
- `time = 10.0;` - compile error (if uniforms are read-only)
- Uniforms cannot be modified

**edge-in-write-error.glsl**: Test writing to in - compile error
- `in vec3 position;`
- `position = vec3(0.0);` - compile error
- Input variables cannot be modified

**edge-out-read-error.glsl**: Test reading from out - compile error (if applicable)
- `out vec4 color;`
- `vec4 c = color;` - may be error depending on stage
- Output variable read restrictions

**edge-no-storage-qualifier.glsl**: Test default storage qualifier
- `float global_var;` - no qualifier
- Default behavior (no linkage, local memory)
- Default qualifier for globals vs locals

**edge-multiple-qualifiers-error.glsl**: Test multiple storage qualifiers - compile error
- `const uniform float x;` - compile error (only one qualifier allowed)
- `in out vec3 v;` - compile error
- At most one storage qualifier per variable

**edge-empty-declaration.glsl**: Test empty declarations
- Empty declarations (if applicable)
- `float;` - may be error or have special meaning

## Implementation Notes

1. **Test Format**: Follow the exact format from other test suites with:
   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:
   - All storage qualifiers (`const`, `uniform`, `in`, `out`, `buffer`, `shared`)
   - All global types (scalars, vectors, matrices, arrays, structs)
   - Initialization rules (constant expressions, undefined values)
   - Scope rules (global scope, shadowing, nested scopes)
   - Access restrictions (read-only, write-only)
   - Shared globals (for GLSL)
   - Error cases (redeclaration, multiple qualifiers, const write)

3. **Key Differences from Local Variables**:
   - Globals have global scope (visible everywhere)
   - Globals can have storage qualifiers (`uniform`, `in`, `out`, etc.)
   - Globals can be shared across shaders (uniforms)
   - Globals without qualifiers have undefined initial values
   - Globals can be accessed from any function

4. **Expected Failures**: These tests are expected to fail initially, especially:
   - Storage qualifiers (`uniform`, `in`, `out`, `buffer`, `shared`)
   - Global scope management
   - Const variable enforcement
   - Read-only restrictions (uniform, in)
   - Shared globals across shaders
   - Proper initialization handling

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/float/op-add.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/functions/scope-global.glsl`
   - GLSL spec: `variables.adoc` - Storage Qualifiers, Scoping sections

## Files to Create

Create 50 test files in the flat `globals/` directory structure above, with each file containing 3-10 test functions following the established pattern. All files use the prefix naming convention:

- `declare-*` for declarations
- `initialize-*` for initialization
- `access-*` for accessing globals
- `scope-*` for scope rules
- `type-*` for type-specific tests
- `const-*` for const qualifier tests
- `uniform-*` for uniform tests
- `in-*` for input tests
- `out-*` for output tests
- `shared-*` for shared globals
- `edge-*` for edge cases

## GLSL Spec References

- **variables.adoc**: Storage Qualifiers (lines 1834-1933), Scoping (lines 1403-1831), Constant Qualifier (lines 1936-1978), Constant Expressions (lines 1980-2108)
- Key sections:
  - Global scope rules
  - Storage qualifiers (`const`, `uniform`, `in`, `out`, `buffer`, `shared`)
  - Initialization rules (constant expressions, undefined values)
  - Shared globals (for GLSL)
  - Read-only restrictions
  - Scope and shadowing rules






