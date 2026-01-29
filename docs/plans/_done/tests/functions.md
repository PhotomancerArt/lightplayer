# Plan: Create Comprehensive Function Tests

## Overview

Create a complete test suite for GLSL user-defined functions in `lightplayer/crates/lp-glsl-filetests/filetests/functions/` following the flat naming convention with prefixes. These tests will comprehensively cover the GLSL function specification including function declarations, definitions, calls, parameter qualifiers (`in`, `out`, `inout`), return types, overloading, and edge cases. These tests are expected to fail initially, serving as a specification for implementing function support in the compiler.

## Directory Structure

Following the flat naming convention with prefixes, create tests in a single `functions/` directory:

```javascript
functions/
├── declare-prototype.glsl          (function prototype declarations)
├── define-simple.glsl              (simple function definitions)
├── call-simple.glsl                (simple function calls)
├── param-in.glsl                   (in parameters - default, explicit)
├── param-out.glsl                  (out parameters - copy out only)
├── param-inout.glsl                (inout parameters - copy in and out)
├── param-const.glsl                (const parameters)
├── param-mixed.glsl                 (functions with mixed parameter qualifiers)
├── return-void.glsl                 (void return type)
├── return-scalar.glsl               (scalar return types: float, int, uint, bool)
├── return-vector.glsl               (vector return types: vec2, vec3, vec4, etc.)
├── return-matrix.glsl               (matrix return types: mat2, mat3, mat4)
├── return-array.glsl                (array return types)
├── return-struct.glsl               (struct return types)
├── overload-same-name.glsl          (function overloading - same name, different types)
├── overload-resolution.glsl        (overload resolution with conversions)
├── overload-ambiguous.glsl          (ambiguous overload errors)
├── forward-declare.glsl             (forward declarations before use)
├── recursive-static-error.glsl      (static recursion detection - should error)
├── call-order.glsl                  (argument evaluation order - left to right)
├── call-multiple.glsl               (multiple calls to same function)
├── call-nested.glsl                 (nested function calls)
├── call-return-value.glsl           (using return values in expressions)
├── param-array.glsl                 (array parameters)
├── param-struct.glsl                (struct parameters)
├── param-unnamed.glsl                (unnamed parameters)
├── param-default-in.glsl            (default parameter qualifier is in)
├── return-early.glsl                (early return statements)
├── return-multiple.glsl              (multiple return paths)
├── scope-local.glsl                 (local variable scope in functions)
├── scope-global.glsl                (global variable access in functions)
├── main-entry.glsl                  (main() function requirements)
├── main-void.glsl                   (main() must return void)
├── main-no-params.glsl              (main() must take no parameters)
├── edge-out-uninitialized.glsl      (out parameters start uninitialized)
├── edge-out-not-read.glsl            (out parameters don't need to be read)
├── edge-inout-both.glsl              (inout parameters copied in and out)
├── edge-const-out-error.glsl         (const with out/inout - compile error)
├── edge-lvalue-out.glsl              (out/inout require lvalues - compile error)
├── edge-array-size-match.glsl        (array parameter size must match)
├── edge-return-type-match.glsl       (return type must match declaration)
└── edge-void-return-value.glsl       (void function cannot return value)
```

## Test File Patterns

Each test file should follow the pattern from other test suites:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

float test_function_operation_name() {
    // Helper function definition
    float helper(float x) {
        return x * 2.0;
    }

    // Test implementation
    return helper(5.0);
    // Should be 10.0
}

// run: test_function_operation_name() ~= 10.0
```

## Key Test Categories

### 1. Function Declarations and Definitions

**declare-prototype.glsl**: Test function prototype declarations

- `float func(float x);` - simple prototype
- `void func();` - void function prototype
- `vec4 func(vec4 a, vec4 b);` - prototype with multiple parameters
- Prototypes allow forward declaration before definition
- Multiple prototypes allowed (must match)

**define-simple.glsl**: Test simple function definitions

- `float func(float x) { return x * 2.0; }` - simple definition
- `void func() { }` - void function definition
- Function body with return statements
- Functions can be defined after being called (if prototype exists)

**call-simple.glsl**: Test simple function calls

- `func(5.0)` - call with single argument
- `func(1.0, 2.0)` - call with multiple arguments
- `func()` - call with no arguments
- Function calls in expressions
- Function calls as statements

### 2. Parameter Qualifiers

**param-in.glsl**: Test `in` parameters (default)

- `float func(in float x)` - explicit in qualifier
- `float func(float x)` - implicit in (default)
- `in` parameters are copied in at call time
- `in` parameters can be modified inside function (only affects local copy)
- Test with various types: float, int, vec2, vec3, vec4, etc.

**param-out.glsl**: Test `out` parameters

- `void func(out float x)` - out parameter
- `out` parameters are NOT copied in (start uninitialized)
- `out` parameters are copied out when function returns
- Must assign to `out` parameter before function returns
- Caller must pass lvalue (variable, not expression)
- Test with various types

**param-inout.glsl**: Test `inout` parameters

- `void func(inout float x)` - inout parameter
- `inout` parameters are copied in at call time
- `inout` parameters are copied out when function returns
- Caller must pass lvalue
- Test modifying inout parameter inside function
- Test with various types

**param-const.glsl**: Test `const` parameters

- `float func(const float x)` - const parameter
- `const` prevents modification inside function
- `const` can only be used with `in` parameters (not `out` or `inout`)
- Test that const parameters cannot be assigned to

**param-mixed.glsl**: Test functions with mixed parameter qualifiers

- `float func(in float a, out float b, inout float c)` - mixed qualifiers
- Test various combinations of in, out, inout
- Test order of parameter evaluation
- Test that out parameters are copied back in undefined order

### 3. Return Types

**return-void.glsl**: Test void return type

- `void func() { }` - void function
- `void func() { return; }` - explicit void return
- Void functions cannot return a value
- Void function calls cannot be used in expressions

**return-scalar.glsl**: Test scalar return types

- `float func() { return 1.0; }` - float return
- `int func() { return 42; }` - int return
- `uint func() { return 100u; }` - uint return
- `bool func() { return true; }` - bool return
- Return value must match declared return type (or convertible)

**return-vector.glsl**: Test vector return types

- `vec2 func() { return vec2(1.0, 2.0); }` - vec2 return
- `vec3 func() { return vec3(1.0, 2.0, 3.0); }` - vec3 return
- `vec4 func() { return vec4(1.0, 2.0, 3.0, 4.0); }` - vec4 return
- Test with ivec2, ivec3, ivec4, uvec2, uvec3, uvec4, bvec2, bvec3, bvec4

**return-matrix.glsl**: Test matrix return types

- `mat2 func() { return mat2(1.0); }` - mat2 return
- `mat3 func() { return mat3(1.0); }` - mat3 return
- `mat4 func() { return mat4(1.0); }` - mat4 return
- Test with various matrix constructors

**return-array.glsl**: Test array return types

- `float[3] func() { return float[3](1.0, 2.0, 3.0); }` - array return
- Arrays must be explicitly sized in return type
- Array is returned by value (copied)

**return-struct.glsl**: Test struct return types

- `struct S { float x; float y; };`
- `S func() { return S(1.0, 2.0); }` - struct return
- Struct is returned by value (copied)

### 4. Function Overloading

**overload-same-name.glsl**: Test function overloading

- `float func(float x);` and `int func(int x);` - same name, different types
- `vec2 func(vec2 a);` and `vec3 func(vec3 a);` - vector overloads
- Overloading based on parameter types (not return type)
- Same parameter types with different return types - compile error
- Same parameter types with different qualifiers - compile error

**overload-resolution.glsl**: Test overload resolution

- Exact match preferred over conversions
- Implicit conversions applied to find match
- Best match selected when multiple matches exist
- Test with float/int conversions
- Test with vector conversions

**overload-ambiguous.glsl**: Test ambiguous overload errors

- Multiple functions match with equal "goodness" - compile error
- Test cases that should produce ambiguous call errors

### 5. Function Calls and Execution

**forward-declare.glsl**: Test forward declarations

- Declare prototype before definition
- Call function before definition (with prototype)
- Multiple prototypes allowed (must match)

**recursive-static-error.glsl**: Test static recursion detection

- Direct recursion: `float func() { return func(); }` - should error
- Indirect recursion: `float a() { return b(); } float b() { return a(); }` - should error
- Static recursion is compile-time error

**call-order.glsl**: Test argument evaluation order

- Arguments evaluated left to right, exactly once
- Test with side effects to verify order
- Test that each argument evaluated exactly once

**call-multiple.glsl**: Test multiple calls to same function

- Call same function multiple times
- Verify each call is independent
- Test with different arguments

**call-nested.glsl**: Test nested function calls

- `func1(func2(5.0))` - nested calls
- Deep nesting
- Test with different return types

**call-return-value.glsl**: Test using return values

- `float x = func();` - assign return value
- `float y = func() * 2.0;` - use in expression
- `if (func() > 0.0) { }` - use in condition
- Return value can be used anywhere expression is valid

### 6. Parameter Types

**param-array.glsl**: Test array parameters

- `void func(float arr[5])` - array parameter
- Array size must match exactly
- Array passed by name (without brackets)
- Test with various array sizes and types
- Test modifying array elements inside function

**param-struct.glsl**: Test struct parameters

- `struct S { float x; float y; };`
- `void func(S s)` - struct parameter
- Struct passed by value (copied)
- Test with in, out, inout struct parameters

**param-unnamed.glsl**: Test unnamed parameters

- `void func(float, int)` - unnamed parameters
- Unnamed parameters valid in prototypes and definitions
- Useful for function matching without naming unused parameters

**param-default-in.glsl**: Test default parameter qualifier

- `float func(float x)` - default is `in`
- Equivalent to `float func(in float x)`
- Test that default behavior matches explicit `in`

### 7. Return Statements

**return-early.glsl**: Test early return statements

- `if (condition) return value;` - early return
- Multiple return paths in function
- Return causes immediate exit

**return-multiple.glsl**: Test multiple return paths

- Function with multiple return statements
- Different return paths return different values
- All paths must return compatible type

### 8. Scope

**scope-local.glsl**: Test local variable scope

- Variables declared in function are local
- Local variables shadow globals
- Local variables not accessible outside function

**scope-global.glsl**: Test global variable access

- Functions can access global variables
- Global variables accessible from any function
- Test reading and writing globals

### 9. Main Function

**main-entry.glsl**: Test main() function requirements

- `void main() { }` - entry point
- Main function is required (at link time for GLSL, compile time for ESSL)
- Main is entry point for shader execution

**main-void.glsl**: Test main() return type

- `void main() { }` - must return void
- `int main() { }` - compile error
- `float main() { }` - compile error

**main-no-params.glsl**: Test main() parameters

- `void main() { }` - no parameters allowed
- `void main(float x) { }` - compile error
- `void main(int argc) { }` - compile error

### 10. Edge Cases

**edge-out-uninitialized.glsl**: Test out parameter initialization

- `out` parameters start uninitialized in function
- Must assign before function returns
- If not assigned, caller receives uninitialized value (undefined behavior)

**edge-out-not-read.glsl**: Test that out parameters don't need to be read

- Can assign to out parameter without reading it first
- Out parameter doesn't need initial value from caller

**edge-inout-both.glsl**: Test inout parameter behavior

- `inout` parameter copied in at start
- `inout` parameter copied out at end
- Modifications inside function affect caller's variable

**edge-const-out-error.glsl**: Test const with out/inout error

- `void func(const out float x)` - compile error
- `const` cannot be used with `out` or `inout`
- Only `const in` is allowed

**edge-lvalue-out.glsl**: Test that out/inout require lvalues

- `func(5.0)` where parameter is `out float` - compile error
- Must pass variable, not expression
- Test various non-lvalue expressions

**edge-array-size-match.glsl**: Test array parameter size matching

- `void func(float arr[5])` called with `float arr[3]` - compile error
- Array size must match exactly
- Test with various mismatches

**edge-return-type-match.glsl**: Test return type matching

- Function definition return type must match prototype
- Return statement value must be convertible to return type
- Test type mismatches

**edge-void-return-value.glsl**: Test void return restrictions

- `void func() { return 5.0; }` - compile error
- Void functions cannot return values
- `return;` without value is correct for void

## Implementation Notes

1. **Test Format**: Follow the exact format from other test suites with:

   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:

   - All parameter qualifiers (`in`, `out`, `inout`, `const`)
   - All return types (void, scalars, vectors, matrices, arrays, structs)
   - Function overloading and resolution
   - Forward declarations
   - Argument evaluation order
   - Scope rules
   - Main function requirements
   - Error cases (recursion, ambiguous overloads, type mismatches)

3. **Key Differences from Built-in Functions**:

   - User-defined functions support `out` and `inout` parameters
   - User-defined functions can be overloaded
   - User-defined functions can return arrays and structs
   - User-defined functions can be forward declared
   - User-defined functions cannot be recursive

4. **Expected Failures**: These tests are expected to fail initially, especially:

   - `out` and `inout` parameter handling
   - Function overloading and resolution
   - Array and struct parameters/returns
   - Forward declarations
   - Static recursion detection
   - Proper argument evaluation order
   - Return type checking

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/float/op-add.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec2/fn-length.glsl`
   - GLSL spec: `statements.adoc` - Function Definitions section

## Files to Create

Create 50 test files in the flat `functions/` directory structure above, with each file containing 3-10 test functions following the established pattern. All files use the prefix naming convention:

- `declare-*` for declarations
- `define-*` for definitions
- `call-*` for function calls
- `param-*` for parameter qualifiers
- `return-*` for return types
- `overload-*` for overloading
- `scope-*` for scope rules
- `main-*` for main function
- `edge-*` for edge cases

## GLSL Spec References

- **statements.adoc**: Function Definitions (lines 70-523), Function Calling Conventions (lines 324-437)
- **grammar.adoc**: Function grammar rules (lines 292-883)
- Key sections:
  - Function prototypes and definitions
  - Parameter qualifiers (`in`, `out`, `inout`, `const`)
  - Return types and return statements
  - Function overloading and resolution
  - Argument evaluation order
  - Static recursion restrictions
  - Main function requirements





