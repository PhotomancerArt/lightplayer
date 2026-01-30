# Plan: Document Existing Function Tests

## Overview

This plan documents the existing function tests in `lightplayer/crates/lp-glsl-filetests/filetests/functions/`. These tests comprehensively cover GLSL user-defined functions including declarations, definitions, calls, parameter qualifiers, return types, overloading, and edge cases.

## Existing Test Structure

The function tests are organized in a flat structure with 42 test files:

```
functions/
├── call-multiple.glsl
├── call-nested.glsl
├── call-order.glsl
├── call-return-value.glsl
├── call-simple.glsl
├── declare-prototype.glsl
├── define-simple.glsl
├── edge-array-size-match.glsl
├── edge-const-out-error.glsl
├── edge-inout-both.glsl
├── edge-lvalue-out.glsl
├── edge-out-not-read.glsl
├── edge-out-uninitialized.glsl
├── edge-return-type-match.glsl
├── edge-void-return-value.glsl
├── forward-declare.glsl
├── main-entry.glsl
├── main-no-params.glsl
├── main-void.glsl
├── overload-ambiguous.glsl
├── overload-resolution.glsl
├── overload-same-name.glsl
├── param-array.glsl
├── param-const.glsl
├── param-default-in.glsl
├── param-in.glsl
├── param-inout.glsl
├── param-mixed.glsl
├── param-out.glsl
├── param-struct.glsl
├── param-unnamed.glsl
├── recursive-static-error.glsl
├── return-array.glsl
├── return-early.glsl
├── return-matrix.glsl
├── return-multiple.glsl
├── return-scalar.glsl
├── return-struct.glsl
├── return-vector.glsl
├── return-void.glsl
├── scope-global.glsl
└── scope-local.glsl
```

## Test Coverage

### Function Declarations and Definitions

**declare-prototype.glsl**: Function prototype declarations
- Simple prototypes
- Prototypes with parameters
- Forward declarations

**define-simple.glsl**: Simple function definitions
- Function bodies
- Return statements
- Basic function definitions

### Function Calls

**call-simple.glsl**: Simple function calls
- Calls with single argument
- Calls with multiple arguments
- Calls with no arguments

**call-multiple.glsl**: Multiple calls to same function
- Same function called multiple times
- Independent calls
- Different arguments

**call-nested.glsl**: Nested function calls
- Functions calling other functions
- Deep nesting
- Return value usage

**call-order.glsl**: Argument evaluation order
- Left to right evaluation
- Exactly once evaluation
- Side effects

**call-return-value.glsl**: Using return values
- Assign return value
- Use in expressions
- Use in conditions

### Parameter Qualifiers

**param-in.glsl**: In parameters (default)
- Explicit `in` qualifier
- Implicit `in` (default)
- Copy in behavior

**param-out.glsl**: Out parameters
- `out` qualifier
- Copy out only
- Uninitialized start

**param-inout.glsl**: Inout parameters
- `inout` qualifier
- Copy in and out
- Modification behavior

**param-const.glsl**: Const parameters
- `const` qualifier
- Read-only restriction
- Only with `in`

**param-mixed.glsl**: Mixed parameter qualifiers
- Functions with in, out, inout
- Various combinations
- Parameter order

**param-default-in.glsl**: Default parameter qualifier
- Default is `in`
- Equivalent to explicit `in`

**param-unnamed.glsl**: Unnamed parameters
- Parameters without names
- Prototypes and definitions

**param-array.glsl**: Array parameters
- Array size matching
- Array passing

**param-struct.glsl**: Struct parameters
- Struct passing
- Struct copying

### Return Types

**return-void.glsl**: Void return type
- Void functions
- Return without value

**return-scalar.glsl**: Scalar return types
- float, int, uint, bool returns
- Return value matching

**return-vector.glsl**: Vector return types
- vec2, vec3, vec4 returns
- ivec, uvec, bvec returns

**return-matrix.glsl**: Matrix return types
- mat2, mat3, mat4 returns
- Matrix return values

**return-array.glsl**: Array return types
- Array returns
- Array size requirements

**return-struct.glsl**: Struct return types
- Struct returns
- Struct copying

**return-early.glsl**: Early return statements
- Return before end of function
- Multiple return paths

**return-multiple.glsl**: Multiple return paths
- Different returns in different paths
- All paths return compatible type

### Function Overloading

**overload-same-name.glsl**: Function overloading
- Same name, different parameter types
- Overload resolution

**overload-resolution.glsl**: Overload resolution
- Exact match preferred
- Implicit conversions
- Best match selection

**overload-ambiguous.glsl**: Ambiguous overload errors
- Multiple matches with equal goodness
- Compile error

### Scope

**scope-local.glsl**: Local variable scope
- Variables in functions
- Local scope rules
- Shadowing

**scope-global.glsl**: Global variable access
- Accessing globals from functions
- Reading and writing globals

### Main Function

**main-entry.glsl**: Main function requirements
- Entry point
- Main function definition

**main-void.glsl**: Main return type
- Must return void
- Void return requirement

**main-no-params.glsl**: Main parameters
- No parameters allowed
- Empty parameter list

### Forward Declarations

**forward-declare.glsl**: Forward declarations
- Prototype before definition
- Call before definition

### Edge Cases

**edge-out-uninitialized.glsl**: Out parameter initialization
- Out parameters start uninitialized
- Must assign before return

**edge-out-not-read.glsl**: Out parameters don't need to be read
- Can assign without reading
- No initial value needed

**edge-inout-both.glsl**: Inout parameter behavior
- Copied in at start
- Copied out at end

**edge-const-out-error.glsl**: Const with out/inout error
- `const out` - compile error
- `const inout` - compile error

**edge-lvalue-out.glsl**: Out/inout require lvalues
- Must pass variable
- Cannot pass expression

**edge-array-size-match.glsl**: Array parameter size matching
- Array size must match exactly
- Size mismatch errors

**edge-return-type-match.glsl**: Return type matching
- Return type must match declaration
- Type conversion

**edge-void-return-value.glsl**: Void return restrictions
- Void functions cannot return value
- Return without value

**recursive-static-error.glsl**: Static recursion detection
- Direct recursion - compile error
- Indirect recursion - compile error

## GLSL Spec References

- **statements.adoc**: Function Definitions (lines 70-523), Function Calling Conventions (lines 324-437)
- Key sections:
  - Function prototypes and definitions
  - Parameter qualifiers (in, out, inout, const)
  - Return types and return statements
  - Function overloading and resolution
  - Argument evaluation order
  - Static recursion restrictions
  - Main function requirements






