# Plan: Create Comprehensive Array Tests

## Overview

Create a complete test suite for GLSL arrays in `lightplayer/crates/lp-glsl-filetests/filetests/arrays/` following the flat naming convention with prefixes. These tests will comprehensively cover the GLSL array specification including explicitly-sized arrays, array constructors, indexing, assignment, and operations. These tests are expected to fail initially, serving as a specification for implementing array support in the compiler.

## Directory Structure

Following the flat naming convention with prefixes, create tests in a single `arrays/` directory:

```javascript
arrays/
├── declare-explicit.glsl        (float[5], int[3], vec4[2] - explicitly sized)
├── declare-unsized.glsl         (float[], int[] - unsized arrays)
├── declare-multidim.glsl        (float[3][2], vec4[2][3] - multi-dimensional)
├── constructor-explicit.glsl    (float[3](1.0, 2.0, 3.0) - explicit size constructor)
├── constructor-inferred.glsl     (float[](1.0, 2.0, 3.0) - inferred size constructor)
├── constructor-multidim.glsl     (float[3][2](...) - multi-dimensional constructor)
├── index-constant.glsl          (arr[0], arr[1], arr[2] - constant index)
├── index-variable.glsl          (arr[i] where i is variable - variable index)
├── index-nested.glsl            (arr[i][j] - nested indexing)
├── assign-element.glsl         (arr[0] = value - element assignment)
├── assign-whole.glsl           (arr1 = arr2 - whole array assignment)
├── assign-unsized-error.glsl   (unsized array assignment errors)
├── access-element.glsl          (float x = arr[0] - element access)
├── access-nested.glsl           (float x = arr[i][j] - nested access)
├── length-method.glsl           (arr.length() - array length method)
├── length-constant.glsl         (const int len = arr.length() - constant expression)
├── from-scalar-array.glsl       (float[3](1.0, 2.0, 3.0) - scalar array)
├── from-vector-array.glsl       (vec4[2](vec4(...), vec4(...)) - vector array)
├── from-mixed-array.glsl        (mixed type conversions in arrays)
├── equal-operator.glsl          (arr1 == arr2 - equality operator)
├── not-equal-operator.glsl      (arr1 != arr2 - inequality operator)
├── ternary-operator.glsl        (condition ? arr1 : arr2 - ternary with arrays)
├── function-param.glsl          (function parameters with arrays)
├── function-return.glsl         (function return types with arrays)
├── local-array.glsl             (local variable arrays)
├── global-array.glsl            (global variable arrays)
├── const-array.glsl             (const arrays)
├── initializer-list.glsl        ({1.0, 2.0, 3.0} - initializer list syntax)
├── initializer-nested.glsl      (nested initializer lists)
├── edge-zero-size-error.glsl    (array[0] - compile-time error)
├── edge-negative-index-error.glsl (arr[-1] - compile-time error)
├── edge-out-of-bounds-error.glsl (arr[10] when size is 5 - compile-time error)
├── edge-unsized-index-error.glsl (unsized array with non-constant index - error)
├── edge-runtime-bounds.glsl     (runtime bounds checking - undefined behavior)
└── edge-nested-operations.glsl  (complex nested array operations)
```

## Test File Patterns

Each test file should follow the pattern from other test suites:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

float test_array_operation_name() {
    float arr[3] = float[3](1.0, 2.0, 3.0);
    // Test implementation
    return result;
    // Should be expected_value
}

// run: test_array_operation_name() ~= expected_value
```

## Key Test Categories

### 1. Array Declarations

**declare-explicit.glsl**: Test explicitly-sized array declarations

- `float arr[5];` - array of 5 floats
- `int arr[3];` - array of 3 ints
- `vec4 arr[2];` - array of 2 vec4s
- `bool arr[10];` - array of 10 bools
- Test various base types

**declare-unsized.glsl**: Test unsized array declarations

- `float arr[];` - unsized array (valid in GLSL, illegal in ESSL without initializer)
- `int arr[];` - unsized array
- Test that unsized arrays must be initialized or redeclared with size

**declare-multidim.glsl**: Test multi-dimensional array declarations

- `float arr[3][2];` - 3x2 array of floats
- `vec4 arr[2][3];` - 2x3 array of vec4s
- `int arr[4][5][6];` - 3D array
- Test that inner dimensions iterate faster in memory

### 2. Array Constructors

**constructor-explicit.glsl**: Test explicit-size array constructors

- `float[3](1.0, 2.0, 3.0)` - constructor with explicit size
- `vec4[2](vec4(1.0), vec4(2.0))` - constructor with explicit size
- Must have exactly the same number of arguments as size

**constructor-inferred.glsl**: Test inferred-size array constructors

- `float[](1.0, 2.0, 3.0)` - constructor with inferred size
- `int[](1, 2, 3, 4, 5)` - constructor with inferred size
- Size is determined by number of arguments

**constructor-multidim.glsl**: Test multi-dimensional array constructors

- `float[3][2](float[2](1.0, 2.0), float[2](3.0, 4.0), float[2](5.0, 6.0))`
- Optional sizes in any dimension
- Size deduction for unsized dimensions

### 3. Array Indexing

**index-constant.glsl**: Test constant index access

- `arr[0]`, `arr[1]`, `arr[2]` - constant indices
- Compile-time bounds checking
- Test all valid indices for various array sizes

**index-variable.glsl**: Test variable index access

- `arr[i]` where `i` is a variable
- Runtime bounds checking (undefined behavior if out of bounds)
- Test with computed indices

**index-nested.glsl**: Test nested array indexing

- `arr[i][j]` - multi-dimensional indexing
- `arr[i][j][k]` - 3D indexing
- Test various nesting levels

### 4. Array Assignment

**assign-element.glsl**: Test element assignment

- `arr[0] = value;` - assign to single element
- `arr[i] = value;` - assign with variable index
- Verify other elements unchanged

**assign-whole.glsl**: Test whole array assignment

- `arr1 = arr2;` - assign entire array
- Both arrays must be same size and type
- Both arrays must be explicitly sized
- Test with various types (scalar, vector, etc.)

**assign-unsized-error.glsl**: Test unsized array assignment errors

- Cannot assign to unsized array
- Cannot assign from unsized array
- Compile-time errors for these cases

### 5. Array Access

**access-element.glsl**: Test element access

- `float x = arr[0];` - read element
- `vec4 v = arr[1];` - read vector element
- Test reading from various positions

**access-nested.glsl**: Test nested array access

- `float x = arr[i][j];` - read from multi-dimensional array
- Test various nesting patterns

### 6. Array Length Method

**length-method.glsl**: Test `.length()` method

- `arr.length()` - returns number of elements
- Returns `int` type
- Test with various array sizes

**length-constant.glsl**: Test `.length()` as constant expression

- `const int len = arr.length();` - constant expression when array is compile-time sized
- Can be used in constant expressions
- Test in various constant contexts

### 7. Array Constructors from Values

**from-scalar-array.glsl**: Test scalar array constructors

- `float[3](1.0, 2.0, 3.0)` - from scalars
- `int[5](1, 2, 3, 4, 5)` - from integers
- Test various scalar types

**from-vector-array.glsl**: Test vector array constructors

- `vec4[2](vec4(1.0), vec4(2.0))` - from vectors
- `vec2[3](vec2(1.0, 2.0), vec2(3.0, 4.0), vec2(5.0, 6.0))` - from vectors
- Test various vector types

**from-mixed-array.glsl**: Test arrays with type conversions

- `float[3](1, 2, 3)` - int to float conversion
- `int[2](1.5, 2.7)` - float to int conversion (truncation)
- Test various type conversions

### 8. Array Operators

**equal-operator.glsl**: Test `==` operator on arrays

- `arr1 == arr2` - equality comparison
- Both arrays must be same size and type
- Returns `bool` (true if all elements equal)
- Cannot contain opaque types

**not-equal-operator.glsl**: Test `!=` operator on arrays

- `arr1 != arr2` - inequality comparison
- Both arrays must be same size and type
- Returns `bool` (true if any element differs)

**ternary-operator.glsl**: Test ternary operator with arrays

- `condition ? arr1 : arr2` - ternary selection
- Both arrays must be same size and type
- Test with various conditions

### 9. Arrays in Functions

**function-param.glsl**: Test array function parameters

- `void func(float arr[5])` - array parameter
- Must be explicitly sized
- Cannot be unsized or runtime-sized
- Test passing arrays to functions

**function-return.glsl**: Test array function return types

- `float[5] func()` - return array type
- Must be explicitly sized
- Cannot be unsized or runtime-sized
- Test returning arrays from functions

### 10. Array Scope and Storage

**local-array.glsl**: Test local variable arrays

- Arrays declared in function scope
- Test initialization, assignment, access
- Test lifetime and scope rules

**global-array.glsl**: Test global variable arrays

- Arrays declared at global scope
- Test initialization, assignment, access
- Test shared between functions

**const-array.glsl**: Test const arrays

- `const float arr[3] = float[3](1.0, 2.0, 3.0);`
- Must be initialized
- Elements cannot be modified
- Test in constant expressions

### 11. Array Initialization

**initializer-list.glsl**: Test initializer list syntax

- `float arr[3] = {1.0, 2.0, 3.0};` - initializer list
- `vec4 arr[2] = {vec4(1.0), vec4(2.0)};` - initializer list with constructors
- Test various types

**initializer-nested.glsl**: Test nested initializer lists

- `float arr[3][2] = {{1.0, 2.0}, {3.0, 4.0}, {5.0, 6.0}};`
- Multi-dimensional initialization
- Test various nesting levels

### 12. Edge Cases and Errors

**edge-zero-size-error.glsl**: Test zero-size array error

- `float arr[0];` - compile-time error
- Array size must be greater than zero

**edge-negative-index-error.glsl**: Test negative index error

- `arr[-1]` - compile-time error if constant
- Negative constant indices are compile-time errors

**edge-out-of-bounds-error.glsl**: Test out-of-bounds index error

- `arr[10]` when size is 5 - compile-time error if constant
- Constant indices >= size are compile-time errors

**edge-unsized-index-error.glsl**: Test unsized array indexing errors

- Unsized arrays can only be indexed with constant expressions
- Variable indexing on unsized arrays is compile-time error

**edge-runtime-bounds.glsl**: Test runtime bounds behavior

- Variable indexing with out-of-bounds values
- Undefined behavior (but should not crash)
- Test with various out-of-bounds scenarios

**edge-nested-operations.glsl**: Test complex nested array operations

- Arrays of arrays of vectors
- Nested indexing and assignment
- Complex expressions involving arrays

## Implementation Notes

1. **Test Format**: Follow the exact format from other test suites with:

   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:

   - All array declaration forms (explicitly-sized, unsized, multi-dimensional)
   - All array constructor forms (explicit size, inferred size, multi-dimensional)
   - Array indexing (constant, variable, nested)
   - Array assignment (element, whole array)
   - Array operators (==, !=, ?:)
   - Array length method
   - Arrays in functions (parameters, return types)
   - Array initialization (constructors, initializer lists)
   - Edge cases and error conditions

3. **Key Array Features**:

   - Arrays are homogeneous (all elements same type)
   - Arrays can be explicitly-sized, unsized, or runtime-sized (runtime-sized only in shader storage blocks)
   - Array size must be constant integral expression > 0
   - Array indexing with constant expressions is bounds-checked at compile time
   - Array indexing with variable expressions has undefined behavior if out of bounds
   - Whole array assignment requires same size and type, both explicitly-sized
   - Arrays cannot contain opaque types for equality/assignment

4. **Expected Failures**: These tests are expected to fail initially, especially:

   - Array constructors
   - Array length method
   - Whole array assignment
   - Array equality operators
   - Multi-dimensional arrays
   - Arrays in function parameters/returns
   - Initializer list syntax

5. **GLSL Spec References**:
   - **variables.adoc**: Arrays section (lines 915-1127)
   - **operators.adoc**: Array constructors (lines 326-370), Structure and Array Operations (lines 624-700)
   - **statements.adoc**: Function parameters and return types with arrays

## Files to Create

Create 35 test files in the flat `arrays/` directory structure above, with each file containing 3-10 test functions following the established pattern. All files use the prefix naming convention:

- `declare-*` for array declarations
- `constructor-*` for array constructors
- `index-*` for array indexing
- `assign-*` for array assignment
- `access-*` for array access
- `length-*` for length method
- `from-*` for constructor forms
- `*-operator.glsl` for operators
- `function-*` for function usage
- `*-array.glsl` for storage/scope
- `initializer-*` for initialization
- `edge-*` for edge cases and errors

## GLSL Spec References

- **variables.adoc**: Arrays (lines 915-1127), Array length() method (lines 1110-1127)
- **operators.adoc**: Array constructors (lines 326-370), Structure and Array Operations (lines 624-700)
- **statements.adoc**: Function definitions with array parameters and return types
