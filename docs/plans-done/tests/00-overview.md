# Type-Based Test Suites - Overview

This directory contains plans for creating comprehensive test suites for GLSL types. These test suites serve as executable specifications for the compiler implementation, validating compliance with the GLSL specification.

## Purpose

These test suites are designed to:

1. **Fully validate the compiler** - Provide comprehensive coverage of all type operations, constructors, conversions, and edge cases
2. **Serve as executable specifications** - Each test encodes expected behavior from the GLSL specification
3. **Guide implementation** - Tests are expected to fail initially, providing clear targets for implementation work
4. **Ensure correctness** - Once passing, these tests ensure the compiler correctly implements type semantics

## Test Types

Test suites are planned for various GLSL types:

- **Boolean vectors** (`bvec2`, `bvec3`, `bvec4`) - Planned
- **Float types** (`float`, `vec2`, `vec3`, `vec4`) - Planned (vec4 replaces existing tests)
- **Integer types** (`int`, `ivec2`, `ivec3`, `ivec4`) - Planned
- **Unsigned integer types** (`uint`, `uvec2`, `uvec3`, `uvec4`) - Planned
- **Matrix types** (`mat2`, `mat3`, `mat4`) - Planned
- **Arrays** - Planned
- **Structures** - Planned
- Other types as needed

## Test Structure

Unlike the existing `vec4/` and `matrix/` test suites which use nested subdirectories (e.g., `vec4/relational/`, `vec4/assignment/`), these type-based test suites use a **flat, concise structure** with prefix-based naming. This provides a cleaner, more maintainable organization:

```
bvec4/
├── op-equal.glsl              (operators)
├── op-not-equal.glsl
├── op-not.glsl
├── fn-any.glsl                (built-in functions)
├── fn-all.glsl
├── fn-mix.glsl
├── from-scalar.glsl           (constructors)
├── from-scalars.glsl
├── from-vectors.glsl
├── from-bvec.glsl
├── from-mixed.glsl
├── to-bool.glsl               (conversions)
├── to-int.glsl
├── to-uint.glsl
├── to-float.glsl
├── to-ivec.glsl
├── to-uvec.glsl
├── to-vec.glsl
├── assign-simple.glsl         (assignments)
├── assign-element.glsl
├── assign-swizzle.glsl
├── access-array.glsl          (component access)
├── access-component.glsl
├── access-swizzle.glsl
├── ctrl-if.glsl               (control flow)
├── ctrl-while.glsl
├── ctrl-for.glsl
├── ctrl-do-while.glsl
├── ctrl-ternary.glsl
├── edge-nested.glsl           (edge cases)
├── edge-mixed-components.glsl
├── edge-all-true.glsl
└── edge-all-false.glsl
```

## Naming Conventions

Files use prefix-based naming to organize by category:

- `op-*` - Operators (comparison, logical)
- `fn-*` - Built-in functions (`any()`, `all()`, `not()`, `equal()`, `notEqual()`, `mix()`)
- `from-*` - Constructors (scalar broadcast, multiple scalars, vectors, shortening, identity, mixed types)
- `to-*` - Conversions (to scalar types, to vector types)
- `assign-*` - Assignment operations (simple, element, swizzle)
- `access-*` - Component access (array indexing, component names, swizzling)
- `ctrl-*` - Control flow (if, while, for, do-while, ternary)
- `edge-*` - Edge cases (nested operations, mixed components, all-true, all-false)

## Current Test Suites

### Boolean Vectors (bvec2, bvec3, bvec4)

Three test suites are planned for boolean vectors, one for each vector size:

1. **bvec2** - 32 test files covering 2-component boolean vectors
2. **bvec3** - 33 test files covering 3-component boolean vectors
3. **bvec4** - 33 test files covering 4-component boolean vectors

Each suite follows the same organizational structure, adapted for the appropriate vector size (component count, swizzle patterns, constructor combinations).

### Float Vectors (vec2, vec3, vec4)

Three test suites are planned for float vectors, one for each vector size:

1. **vec2** - 33 test files covering 2-component float vectors
2. **vec3** - 34 test files covering 3-component float vectors
3. **vec4** - 50 test files covering 4-component float vectors (replaces existing nested structure)

### Arrays

A comprehensive test suite for GLSL arrays:

- **arrays** - 35 test files covering array declarations, constructors, indexing, assignment, and operations

### Structures

A comprehensive test suite for GLSL structures:

- **structs** - 35 test files covering struct definitions, constructors, member access, assignment, and operations

### Functions

A comprehensive test suite for GLSL user-defined functions:

- **functions** - 50 test files covering function declarations, definitions, calls, parameter qualifiers (in, out, inout), return types, overloading, and edge cases

### Globals

A comprehensive test suite for GLSL global variables:

- **globals** - 50 test files covering global declarations, storage qualifiers (const, uniform, in, out, buffer, shared), initialization, scope, access from functions, and edge cases

### Built-ins

A comprehensive test suite for GLSL built-in functions not covered by type tests:

- **builtins** - 65 test files covering math functions (trigonometry, exponential, common), matrix functions, integer/bit manipulation functions, and floating-point pack/unpack functions

### Future Test Suites

Additional type-based test suites will follow the same flat, prefix-based naming structure:

- Integer types (`int`, `ivec2`, `ivec3`, `ivec4`)
- Unsigned integer types (`uint`, `uvec2`, `uvec3`, `uvec4`)
- Matrix types (`mat2`, `mat3`, `mat4`)
- Other types as needed

## Expected Test Status

**These tests are NOT expected to pass initially.** They serve as:

- **Specification tests** - Define expected behavior from the GLSL spec
- **Implementation targets** - Provide clear goals for compiler development
- **Validation suite** - Once implemented, ensure correctness

Expected initial failures include:

- `not()`, `any()`, `all()` built-in functions
- `mix()` with `bvec` arguments
- Some constructor forms (shortening, mixed types)
- Some conversion forms
- Swizzle assignment
- Component-wise operations

## GLSL Specification Reference

These test suites are based on the GLSL specification located at:

**`/Users/yona/dev/photomancer/glsl-spec/chapters`**

Key specification sections:

- **operators.adoc**: Constructors (lines 171-229), Vector components (lines 427-579), Equality operators (lines 885-907)
- **builtinfunctions.adoc**: Relational functions (lines 1228-1312), `any()`, `all()`, `not()`, `equal()`, `notEqual()`, `mix()` with `bvec`

## Test File Format

Each test file follows the standard GLSL filetest format:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

bvec4 test_operation_name() {
    // Test implementation
    return result;
    // Should be bvec4(true, false, true, false)
}

// run: test_operation_name() == bvec4(true, false, true, false)
```

## Type-Specific Considerations

Each type has specific semantics and requirements:

### Boolean Vectors (bvec2, bvec3, bvec4)

Boolean vectors have important differences from scalar `bool`:

- Logical operators (`&&`, `||`, `^^`, `!`) work on scalar `bool` only, NOT on `bvec*`
- Use `not(bvec*)` built-in instead of `!bvec*`
- Use `any(bvec*)` or `all(bvec*)` to convert `bvec*` to `bool` for control flow
- `==` and `!=` operators return `bool` (aggregate comparison)
- Use `equal()` and `notEqual()` built-ins for component-wise comparison returning `bvec*`

### Float Vectors (vec2, vec3, vec4)

Float vectors have specific considerations:

- Floating-point precision and rounding behavior
- NaN and Inf propagation through operations
- Approximate equality testing (use `~=` operator)
- Geometric functions (length, distance, dot, normalize, reflect, refract, faceforward)
- Component-wise operations

### Arrays

Arrays have specific considerations:

- Explicitly-sized, unsized, and runtime-sized arrays
- Array constructors with explicit or inferred sizes
- Multi-dimensional arrays (arrays of arrays)
- Array length method
- Whole array assignment requires same size and type
- Arrays cannot contain opaque types for equality/assignment

### Structures

Structures have specific considerations:

- Struct constructors require exactly one argument per member
- Member initialization in declaration order
- Nested structs and structs with array members
- Struct equality compares all members recursively
- Anonymous and embedded structs are not supported
- Member types must be already defined (no forward references)

### Functions

Functions have specific considerations:

- Parameter qualifiers: `in` (default, copy in), `out` (copy out only), `inout` (copy in and out)
- `out` parameters start uninitialized and must be assigned before return
- `const` qualifier can only be used with `in` parameters
- Function overloading based on parameter types (not return type)
- Arguments evaluated left to right, exactly once
- Output parameters copied back in undefined order
- Static recursion is not allowed (compile-time error)
- `main()` function must return `void` and take no parameters

### Globals

Global variables have specific considerations:

- Storage qualifiers: `const` (read-only), `uniform` (application linkage), `in` (previous stage), `out` (next stage), `buffer` (shader storage), `shared` (compute shader)
- Default storage qualifier (no qualifier) means local memory with no linkage
- `const` globals must be initialized with constant expressions
- Uninitialized globals (without qualifier) have undefined values at start of main()
- Globals have global scope - visible from all functions
- Local variables can shadow globals (hide them in nested scopes)
- Shared globals (uniforms) must have same type across shaders
- Read-only restrictions: `const`, `uniform`, `in` cannot be written to
- Write-only restrictions: `out` may have read restrictions depending on stage

### Built-ins

Built-in functions have specific considerations:

- Math functions: trigonometry (sin, cos, tan, asin, acos, atan, hyperbolic variants), exponential (pow, exp, log, sqrt, inversesqrt), common (sign, floor, ceil, round, fract, mod)
- Matrix functions: component-wise multiply, outer product, transpose, determinant, inverse
- Integer/bit functions: add/subtract with carry/borrow, multiply extended, bitfield operations, bit counting, find LSB/MSB
- Pack/unpack functions: pack/unpack normalized values, half precision, double precision
- Component-wise operations: most functions operate component-wise on vectors
- Domain restrictions: many functions have undefined behavior for certain input ranges
- NaN and Inf handling: propagation through operations, detection functions (isnan, isinf)
- Precision: operations have effective precision qualification based on input precision

### Other Types

Future test suites will document type-specific considerations for:

- Integer overflow/underflow semantics
- Matrix operations and transformations
- Type conversion rules and edge cases

## Implementation Plans

Detailed plans for each test suite:

### Boolean Vectors

- [`bvec2.md`](./bvec2.md) - 2-component boolean vector tests
- [`bvec3.md`](./bvec3.md) - 3-component boolean vector tests
- [`bvec4.md`](./bvec4.md) - 4-component boolean vector tests

### Float Vectors

- [`vec2.md`](./vec2.md) - 2-component float vector tests
- [`vec3.md`](./vec3.md) - 3-component float vector tests
- [`vec4.md`](./vec4.md) - 4-component float vector tests (replaces existing nested structure)

### Arrays

- [`arrays.md`](./arrays.md) - Comprehensive array tests

### Structures

- [`structs.md`](./structs.md) - Comprehensive struct tests

### Functions

- [`functions-2.md`](functions-2.md) - Document existing function tests (42 test files)

### Globals

- [`globals-2.md`](globals-2.md) - Document existing global variable tests (45 test files)

### Built-ins

- [`builtins-2.md`](builtins-2.md) - Document existing built-in function tests (50+ test files)
- [`builtins-integer.md`](builtins-integer.md) - Integer/bit function tests (25 test files)
- [`builtins-images.md`](builtins-images.md) - Image function tests (30 test files)
- [`builtins-atomics.md`](builtins-atomics.md) - Atomic function tests (30 test files)
- [`builtins-geometry.md`](builtins-geometry.md) - Geometry shader function tests (15 test files)
- [`builtins-fragment.md`](builtins-fragment.md) - Fragment processing function tests (18 test files)
- [`builtins-noise.md`](builtins-noise.md) - Noise function tests (15 test files)
- [`builtins-barriers.md`](builtins-barriers.md) - Barrier function tests (18 test files)
- [`builtins-subpass.md`](builtins-subpass.md) - Subpass input function tests (10 test files)

### Control Flow

- [`control-flow-2.md`](control-flow-2.md) - Document existing control flow tests (if/else/loops/break/continue/return)
- [`control-flow-switch.md`](control-flow-switch.md) - Switch statement tests (28 test files)
- [`control-flow-discard.md`](control-flow-discard.md) - Discard statement tests (15 test files)

### Operators

- [`operators-2.md`](operators-2.md) - Document existing operator tests (increment/decrement)

### Qualifiers

- [`qualifiers-layout.md`](qualifiers-layout.md) - Layout qualifier tests (50 test files)
- [`qualifiers-interpolation.md`](qualifiers-interpolation.md) - Interpolation qualifier tests (28 test files)
- [`qualifiers-memory.md`](qualifiers-memory.md) - Memory qualifier tests (30 test files)
- [`qualifiers-invariant.md`](qualifiers-invariant.md) - Invariant qualifier tests (25 test files)
- [`qualifiers-precise.md`](qualifiers-precise.md) - Precise qualifier tests (22 test files)
- [`qualifiers-precision.md`](qualifiers-precision.md) - Precision qualifier tests (27 test files)

### Interface Blocks

- [`interface-blocks.md`](interface-blocks.md) - Interface block tests (30 test files)

### Textures

- [`textures.md`](textures.md) - Texture function tests (70+ test files)

### Future Plans

Additional type-based test suite plans will be added as they are developed.

Each plan includes:

- Complete file listing with descriptions
- Test categories and expected coverage
- Implementation notes
- Reference file patterns
- GLSL spec references
