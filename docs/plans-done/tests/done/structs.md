# Plan: Create Comprehensive Struct Tests

## Overview

Create a complete test suite for GLSL structures in `lightplayer/crates/lp-glsl-filetests/filetests/structs/` following the flat naming convention with prefixes. These tests will comprehensively cover the GLSL structure specification including struct definitions, constructors, member access, assignment, and operations. These tests are expected to fail initially, serving as a specification for implementing struct support in the compiler.

## Directory Structure

Following the flat naming convention with prefixes, create tests in a single `structs/` directory:

```javascript
structs/
├── define-simple.glsl          (struct with scalar members)
├── define-vector.glsl         (struct with vector members)
├── define-mixed.glsl          (struct with mixed type members)
├── define-nested.glsl          (struct with struct members)
├── define-array-member.glsl   (struct with array members)
├── define-multiple.glsl        (multiple struct definitions)
├── constructor-simple.glsl     (struct constructor with scalars)
├── constructor-vectors.glsl    (struct constructor with vectors)
├── constructor-mixed.glsl      (struct constructor with mixed types)
├── constructor-nested.glsl     (struct constructor with nested structs)
├── constructor-array.glsl      (struct constructor with arrays)
├── access-scalar.glsl          (struct.member - scalar member access)
├── access-vector.glsl          (struct.member - vector member access)
├── access-nested.glsl          (struct.member.member - nested access)
├── access-array-member.glsl    (struct.arr[i] - array member access)
├── assign-simple.glsl          (struct1 = struct2 - whole struct assignment)
├── assign-member.glsl          (struct.member = value - member assignment)
├── assign-nested.glsl          (struct.member.member = value - nested assignment)
├── equal-operator.glsl         (struct1 == struct2 - equality operator)
├── not-equal-operator.glsl    (struct1 != struct2 - inequality operator)
├── ternary-operator.glsl      (condition ? struct1 : struct2 - ternary)
├── function-param.glsl         (function parameters with structs)
├── function-return.glsl        (function return types with structs)
├── local-struct.glsl           (local variable structs)
├── global-struct.glsl          (global variable structs)
├── const-struct.glsl           (const structs)
├── initializer-list.glsl       ({member1, member2} - initializer list syntax)
├── initializer-nested.glsl     (nested initializer lists)
├── edge-empty-error.glsl       (struct with no members - error)
├── edge-anonymous-error.glsl   (anonymous struct - error)
├── edge-embedded-error.glsl    (embedded struct definition - error)
├── edge-forward-ref-error.glsl (forward reference - error)
├── edge-opaque-member-error.glsl (opaque type member - error)
└── edge-complex-nested.glsl    (complex nested struct operations)
```

## Test File Patterns

Each test file should follow the pattern from other test suites:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

struct Point {
    float x;
    float y;
};

float test_struct_operation_name() {
    Point p = Point(1.0, 2.0);
    // Test implementation
    return result;
    // Should be expected_value
}

// run: test_struct_operation_name() ~= expected_value
```

## Key Test Categories

### 1. Struct Definitions

**define-simple.glsl**: Test simple struct definitions
- `struct Point { float x; float y; };` - struct with scalar members
- `struct Color { float r; float g; float b; };` - struct with scalar members
- Test various scalar types (float, int, uint, bool)

**define-vector.glsl**: Test struct definitions with vector members
- `struct Transform { vec3 position; vec3 rotation; };` - struct with vector members
- `struct Color { vec4 rgba; };` - struct with vector members
- Test various vector types (vec2, vec3, vec4, ivec2, etc.)

**define-mixed.glsl**: Test struct definitions with mixed type members
- `struct Light { float intensity; vec3 position; bool enabled; };` - mixed types
- Test combinations of scalars, vectors, and other types

**define-nested.glsl**: Test struct definitions with nested structs
- `struct Point { float x; float y; };`
- `struct Line { Point start; Point end; };` - nested struct
- Test multiple levels of nesting

**define-array-member.glsl**: Test struct definitions with array members
- `struct Buffer { float data[10]; };` - struct with array member
- `struct Matrix { vec4 columns[4]; };` - struct with vector array
- Test various array types and sizes

**define-multiple.glsl**: Test multiple struct definitions
- Multiple structs in same scope
- Struct name visibility and scoping
- Test name conflicts and resolution

### 2. Struct Constructors

**constructor-simple.glsl**: Test struct constructors with scalars
- `Point p = Point(1.0, 2.0);` - constructor with scalar arguments
- One argument per member, in order
- Test various scalar types

**constructor-vectors.glsl**: Test struct constructors with vectors
- `Transform t = Transform(vec3(1.0), vec3(2.0));` - constructor with vectors
- Test various vector types

**constructor-mixed.glsl**: Test struct constructors with mixed types
- `Light l = Light(1.0, vec3(0.0), true);` - constructor with mixed types
- Test type conversions in constructors

**constructor-nested.glsl**: Test struct constructors with nested structs
- `Line l = Line(Point(1.0, 2.0), Point(3.0, 4.0));` - nested constructor
- Test multiple levels of nesting

**constructor-array.glsl**: Test struct constructors with arrays
- `Buffer b = Buffer(float[10](...));` - constructor with array
- Must provide exactly one argument per member (including array members)

### 3. Member Access

**access-scalar.glsl**: Test scalar member access
- `struct.x` - access scalar member
- `struct.member` - access by name
- Test reading and writing scalar members

**access-vector.glsl**: Test vector member access
- `struct.position` - access vector member
- `struct.position.x` - access vector component
- Test reading and writing vector members

**access-nested.glsl**: Test nested member access
- `struct.member.member` - nested access
- `struct.member.x` - access nested struct member
- Test multiple levels of nesting

**access-array-member.glsl**: Test array member access
- `struct.arr[i]` - access array member element
- `struct.arr[i].x` - access vector component in array
- Test various array access patterns

### 4. Struct Assignment

**assign-simple.glsl**: Test whole struct assignment
- `struct1 = struct2;` - assign entire struct
- Both structs must be same type
- Cannot contain opaque types
- Test with various struct types

**assign-member.glsl**: Test member assignment
- `struct.member = value;` - assign to member
- Test with various member types
- Verify other members unchanged

**assign-nested.glsl**: Test nested member assignment
- `struct.member.member = value;` - nested assignment
- Test multiple levels of nesting

### 5. Struct Operators

**equal-operator.glsl**: Test `==` operator on structs
- `struct1 == struct2` - equality comparison
- Both structs must be same type
- Returns `bool` (true if all members equal)
- Cannot contain opaque types
- Members compared recursively

**not-equal-operator.glsl**: Test `!=` operator on structs
- `struct1 != struct2` - inequality comparison
- Both structs must be same type
- Returns `bool` (true if any member differs)

**ternary-operator.glsl**: Test ternary operator with structs
- `condition ? struct1 : struct2` - ternary selection
- Both structs must be same type
- Test with various conditions

### 6. Structs in Functions

**function-param.glsl**: Test struct function parameters
- `void func(Point p)` - struct parameter
- Test passing structs to functions
- Test by value semantics

**function-return.glsl**: Test struct function return types
- `Point func()` - return struct type
- Test returning structs from functions
- Test return value assignment

### 7. Struct Scope and Storage

**local-struct.glsl**: Test local variable structs
- Structs declared in function scope
- Test initialization, assignment, access
- Test lifetime and scope rules

**global-struct.glsl**: Test global variable structs
- Structs declared at global scope
- Test initialization, assignment, access
- Test shared between functions

**const-struct.glsl**: Test const structs
- `const Point p = Point(1.0, 2.0);` - const struct
- Must be initialized
- Members cannot be modified
- Test in constant expressions

### 8. Struct Initialization

**initializer-list.glsl**: Test initializer list syntax
- `Point p = {1.0, 2.0};` - initializer list
- `Light l = {1.0, vec3(0.0), true};` - initializer list with mixed types
- Members initialized in declaration order

**initializer-nested.glsl**: Test nested initializer lists
- `Line l = {{1.0, 2.0}, {3.0, 4.0}};` - nested initializer list
- Test multiple levels of nesting

### 9. Edge Cases and Errors

**edge-empty-error.glsl**: Test empty struct error
- `struct Empty {};` - compile-time error
- Structs must have at least one member

**edge-anonymous-error.glsl**: Test anonymous struct error
- `struct { float x; };` - compile-time error
- Anonymous structures are not supported

**edge-embedded-error.glsl**: Test embedded struct definition error
- `struct Outer { struct Inner { float x; }; };` - compile-time error
- Embedded structure definitions are not supported
- Must define structs separately

**edge-forward-ref-error.glsl**: Test forward reference error
- Using struct type before definition
- Member types must be already defined
- Test forward reference scenarios

**edge-opaque-member-error.glsl**: Test opaque type member restrictions
- Structs cannot contain certain opaque types (in ESSL: atomic_uint, images)
- Test restrictions on opaque type members

**edge-complex-nested.glsl**: Test complex nested struct operations
- Structs containing structs containing arrays
- Deep nesting levels
- Complex expressions involving nested structs

## Implementation Notes

1. **Test Format**: Follow the exact format from other test suites with:
   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:
   - All struct definition forms (simple, with vectors, nested, with arrays)
   - All struct constructor forms (scalars, vectors, mixed, nested, arrays)
   - Member access (scalar, vector, nested, array members)
   - Struct assignment (whole struct, member assignment, nested)
   - Struct operators (==, !=, ?:)
   - Structs in functions (parameters, return types)
   - Struct initialization (constructors, initializer lists)
   - Edge cases and error conditions

3. **Key Struct Features**:
   - Structs are user-defined aggregate types
   - Structs must have at least one member
   - Struct members are initialized in declaration order
   - Struct constructors require exactly one argument per member
   - Struct assignment requires same type, cannot contain opaque types
   - Struct equality compares all members recursively
   - Anonymous and embedded structs are not supported
   - Member types must be already defined (no forward references)

4. **Expected Failures**: These tests are expected to fail initially, especially:
   - Struct constructors
   - Struct equality operators
   - Nested structs
   - Structs with array members
   - Structs in function parameters/returns
   - Initializer list syntax
   - Member access for nested structs

5. **GLSL Spec References**:
   - **variables.adoc**: Structures section (lines 813-912)
   - **operators.adoc**: Structure constructors (lines 297-325), Structure and Array Operations (lines 624-700)
   - **statements.adoc**: Function definitions with struct parameters and return types

## Files to Create

Create 35 test files in the flat `structs/` directory structure above, with each file containing 3-10 test functions following the established pattern. All files use the prefix naming convention:

- `define-*` for struct definitions
- `constructor-*` for struct constructors
- `access-*` for member access
- `assign-*` for struct assignment
- `*-operator.glsl` for operators
- `function-*` for function usage
- `*-struct.glsl` for storage/scope
- `initializer-*` for initialization
- `edge-*` for edge cases and errors

## GLSL Spec References

- **variables.adoc**: Structures (lines 813-912), Structure members (lines 852-912)
- **operators.adoc**: Structure constructors (lines 297-325), Structure and Array Operations (lines 624-700)
- **statements.adoc**: Function definitions with struct parameters and return types

