# Plan: Create Comprehensive mat2 Tests

## Overview

Create a complete test suite for matrix type `mat2` in `lightplayer/crates/lp-glsl-filetests/filetests/mat2/` following the organizational pattern used in `vec4/` and `bool/`. These tests will comprehensively cover the GLSL 2x2 float matrix specification and are expected to fail initially, serving as a specification for implementing the rest of the compiler.

## Directory Structure

Following the flat naming convention with prefixes (like `vec4/` and `bool/`), create tests in a single `mat2/` directory:

```javascript
mat2/
├── op-add.glsl              (mat2 + mat2 -> mat2, component-wise)
├── op-subtract.glsl         (mat2 - mat2 -> mat2, component-wise)
├── op-multiply.glsl         (mat2 * mat2 -> mat2, matrix multiplication)
├── op-multiply-vec.glsl     (mat2 * vec2 -> vec2, matrix-vector multiplication)
├── op-unary-minus.glsl      (-mat2 -> mat2, component-wise negation)
├── op-increment-pre.glsl    (++mat2 -> mat2, pre-increment)
├── op-increment-post.glsl   (mat2++ -> mat2, post-increment)
├── op-decrement-pre.glsl    (--mat2 -> mat2, pre-decrement)
├── op-decrement-post.glsl   (mat2-- -> mat2, post-decrement)
├── op-equal.glsl            (mat2 == mat2 -> bool, aggregate comparison)
├── op-not-equal.glsl        (mat2 != mat2 -> bool, aggregate comparison)
├── fn-transpose.glsl        (transpose(mat2) -> mat2)
├── fn-determinant.glsl      (determinant(mat2) -> float)
├── fn-inverse.glsl          (inverse(mat2) -> mat2)
├── fn-outer-product.glsl    (outerProduct(vec2, vec2) -> mat2)
├── fn-matrix-comp-mult.glsl (matrixCompMult(mat2, mat2) -> mat2)
├── from-scalar.glsl         (mat2(float) - diagonal matrix)
├── from-scalars.glsl        (mat2(float, float, float, float) - column-major)
├── from-vectors.glsl        (mat2(vec2, vec2) - column vectors)
├── from-matrix.glsl         (mat2(mat3), mat2(mat4) - truncation)
├── from-mat.glsl            (mat2(mat2) - identity)
├── to-vec.glsl              (mat2 column extraction: mat2[0], mat2[1])
├── assign-simple.glsl       (mat2 = mat2)
├── assign-element.glsl      (mat2[0][0] = float - element assignment)
├── assign-column.glsl       (mat2[0] = vec2 - column assignment)
├── access-column.glsl       (mat2[0], mat2[1] - column access)
├── access-element.glsl      (mat2[0][0], mat2[1][1] - element access)
├── access-nested.glsl       (mat2[col][row] with computed indices)
├── ctrl-if.glsl             (if (mat2 == mat2) - control flow)
├── ctrl-while.glsl          (while (mat2 != mat2))
├── ctrl-for.glsl            (for (init; mat2 == mat2; update))
├── ctrl-do-while.glsl       (do { } while (mat2 != mat2))
├── ctrl-ternary.glsl        (mat2 == mat2 ? expr1 : expr2)
├── edge-identity.glsl       (identity matrix patterns)
├── edge-zero.glsl           (zero matrix patterns)
├── edge-singular.glsl       (singular matrices - no inverse)
└── edge-large-values.glsl   (matrices with large float values)
```

## Test File Patterns

Each test file should follow the pattern from `vec4/` and `bool/` tests:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

mat2 test_mat_operation_name() {
    // Test implementation
    return result;
    // Should be mat2(expected_col0_x, expected_col0_y, expected_col1_x, expected_col1_y)
}

// run: test_mat_operation_name() ~= mat2(expected_col0_x, expected_col0_y, expected_col1_x, expected_col1_y)
```

Note: Use `~=` for approximate equality due to floating-point precision issues. Matrix literals use column-major order.

## Key Test Categories

### 1. Arithmetic Operators

**op-add.glsl**: Test `+` operator (component-wise)

- `mat2 + mat2` → `mat2` (component-wise addition)
- Test with various matrices

**op-subtract.glsl**: Test `-` operator (component-wise)

- `mat2 - mat2` → `mat2` (component-wise subtraction)
- Test with various matrices

**op-multiply.glsl**: Test `*` operator (matrix multiplication)

- `mat2 * mat2` → `mat2` (matrix multiplication)
- Test matrix multiplication rules
- Test identity matrix properties

**op-multiply-vec.glsl**: Test matrix-vector multiplication

- `mat2 * vec2` → `vec2` (matrix-vector multiplication)
- Test transformation properties

### 2. Unary Operators

**op-unary-minus.glsl**: Test `-` unary operator (component-wise)

- `-mat2` → `mat2` (component-wise negation)
- Test with various matrices

### 3. Increment/Decrement Operators

**op-increment-pre.glsl**: Test `++` pre-increment

- `++mat2` → `mat2` (increment all elements, return new value)
- Must be on lvalue

**op-increment-post.glsl**: Test `++` post-increment

- `mat2++` → `mat2` (return old value, then increment all elements)
- Must be on lvalue

**op-decrement-pre.glsl**: Test `--` pre-decrement

- `--mat2` → `mat2` (decrement all elements, return new value)
- Must be on lvalue

**op-decrement-post.glsl**: Test `--` post-decrement

- `mat2--` → `mat2` (return old value, then decrement all elements)
- Must be on lvalue

### 4. Comparison Operators

**op-equal.glsl**: Test `==` operator (aggregate comparison)

- `mat2 == mat2` → `bool` (true if all elements equal)
- Test with identical matrices, different matrices

**op-not-equal.glsl**: Test `!=` operator (aggregate comparison)

- `mat2 != mat2` → `bool` (true if any element differs)

### 5. Built-in Functions

**fn-transpose.glsl**: Test `transpose()` built-in

- `transpose(mat2)` → `mat2` (transpose matrix)
- Test that transpose(transpose(m)) == m

**fn-determinant.glsl**: Test `determinant()` built-in

- `determinant(mat2)` → `float` (matrix determinant)
- Test determinant properties

**fn-inverse.glsl**: Test `inverse()` built-in

- `inverse(mat2)` → `mat2` (matrix inverse)
- Test that m \* inverse(m) == identity (for invertible matrices)
- Test with singular matrices (undefined behavior)

**fn-outer-product.glsl**: Test `outerProduct()` built-in

- `outerProduct(vec2, vec2)` → `mat2` (outer product of vectors)
- Test that outer product creates expected matrix

**fn-matrix-comp-mult.glsl**: Test `matrixCompMult()` built-in

- `matrixCompMult(mat2, mat2)` → `mat2` (component-wise multiplication)
- Note: Different from matrix multiplication operator

### 6. Constructors

**from-scalar.glsl**: Test scalar constructor (diagonal matrix)

- `mat2(float)` - creates diagonal matrix with float on diagonal, 0 elsewhere
- `mat2(5.0)` → `mat2(5.0, 0.0, 0.0, 5.0)`

**from-scalars.glsl**: Test constructor from scalars (column-major)

- `mat2(float, float, float, float)` - column-major order
- `mat2(a, b, c, d)` → `mat2(a, c, b, d)` where [a, c] is first column, [b, d] is second column

**from-vectors.glsl**: Test constructor from column vectors

- `mat2(vec2, vec2)` - two column vectors
- `mat2(vec2(a, b), vec2(c, d))` → `mat2(a, c, b, d)`

**from-matrix.glsl**: Test constructor from larger matrices (truncation)

- `mat2(mat3)` - truncates upper-left 2x2
- `mat2(mat4)` - truncates upper-left 2x2
- Test component preservation

**from-mat.glsl**: Test identity constructor

- `mat2(mat2)` - identity constructor
- Should preserve all elements

### 7. Conversions

**to-vec.glsl**: Test column extraction

- `mat2[0]` → `vec2` (first column)
- `mat2[1]` → `vec2` (second column)
- Test column vector contents

### 8. Assignment

**assign-simple.glsl**: Test simple assignment

- `mat2 a = mat2(...); mat2 b = a;` - assignment
- Verify independence (modifying one doesn't affect the other)
- Self-assignment: `mat2 = mat2`

**assign-element.glsl**: Test element assignment

- `mat2[0][0] = float` - assign to specific element
- Test all elements: [0][0], [0][1], [1][0], [1][1]
- Verify other elements unchanged

**assign-column.glsl**: Test column assignment

- `mat2[0] = vec2` - assign to entire column
- `mat2[1] = vec2` - assign to entire column
- Test both columns

### 9. Indexing and Access

**access-column.glsl**: Test column access

- `mat2[0]`, `mat2[1]` - column access returns vec2
- Variable indexing: `mat2[i]` where `i` is computed
- Verify correct column vectors

**access-element.glsl**: Test element access

- `mat2[0][0]`, `mat2[0][1]`, `mat2[1][0]`, `mat2[1][1]` - element access
- Verify correct scalar values

**access-nested.glsl**: Test nested/computed indexing

- `mat2[col][row]` with computed col/row indices
- Test bounds and correctness

### 10. Control Flow

**ctrl-if.glsl**: Test `if` statements with mat2

- `if (mat2 == mat2)` - condition using matrix comparison
- Test with equal/unequal matrices

**ctrl-while.glsl**: Test `while` loops with mat2

- `while (mat2 != mat2)` - loop condition

**ctrl-for.glsl**: Test `for` loops with mat2

- `for (init; mat2 == mat2; update)` - for loop condition

**ctrl-do-while.glsl**: Test `do-while` loops with mat2

- `do { } while (mat2 != mat2)` - do-while condition

**ctrl-ternary.glsl**: Test ternary operator with mat2

- `mat2 == mat2 ? expr1 : expr2` - ternary with mat2 condition

### 11. Edge Cases

**edge-identity.glsl**: Test identity matrix patterns

- `mat2(1.0)` - identity matrix
- Matrix multiplication with identity
- Inverse of identity

**edge-zero.glsl**: Test zero matrix patterns

- Zero matrix operations
- Multiplication by zero

**edge-singular.glsl**: Test singular matrices

- Matrices with zero determinant
- Inverse of singular matrices (undefined behavior)

**edge-large-values.glsl**: Test matrices with large values

- Precision issues with large floats
- Overflow in operations

## Implementation Notes

1. **Test Format**: Follow the exact format from `vec4/` and `bool/` tests with:

   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results (use `~=` for approximate equality)
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:

   - All operators from GLSL spec (operators.adoc)
   - All constructor forms (operators.adoc, constructors section)
   - All matrix operations (operators.adoc, matrix operations)
   - All built-in functions (builtinfunctions.adoc: matrix functions)
   - Matrix indexing (column and element access)
   - Control flow requirements (statements.adoc)

3. **Key Differences from vectors**:

   - Column-major storage and indexing
   - Matrix multiplication (not component-wise)
   - Matrix-vector multiplication
   - Matrix-specific built-ins (transpose, determinant, inverse)
   - No component swizzling (like vectors)
   - Column access returns vectors

4. **Expected Failures**: These tests are expected to fail initially, especially:

   - Matrix multiplication operators
   - Built-in functions (transpose, determinant, inverse, etc.)
   - Matrix constructors from scalars/vectors
   - Matrix indexing operations
   - Increment/decrement operators

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/matrix/arithmetic/addition.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/matrix/constructors/from-scalar.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/matrix/indexing/column-access.glsl`

## Files to Create

Create 25 test files in the flat `mat2/` directory structure above, with each file containing 3-10 test functions following the vec4/bool pattern. All files use the prefix naming convention:

- `op-*` for operators (arithmetic, unary, increment/decrement, comparison)
- `fn-*` for built-in functions (transpose, determinant, inverse, outer-product, matrix-comp-mult)
- `from-*` for constructors (from-scalar, from-scalars, from-vectors, from-matrix, from-mat)
- `to-*` for conversions (to-vec)
- `assign-*` for assignments (assign-simple, assign-element, assign-column)
- `access-*` for indexing/access (access-column, access-element, access-nested)
- `ctrl-*` for control flow
- `edge-*` for edge cases

## GLSL Spec References

- **operators.adoc**: Constructors (lines 171-229), Matrix operations (lines 700-884), Conversions (lines 908-1100)
- **builtinfunctions.adoc**: Matrix functions (lines 1451-1600)
