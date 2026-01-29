# Plan: Create Comprehensive mat3 Tests

## Overview

Create a complete test suite for matrix type `mat3` in `lightplayer/crates/lp-glsl-filetests/filetests/mat3/` following the organizational pattern used in `vec4/` and `bool/`. These tests will comprehensively cover the GLSL 3x3 float matrix specification and are expected to fail initially, serving as a specification for implementing the rest of the compiler.

## Directory Structure

Following the flat naming convention with prefixes (like `vec4/` and `bool/`), create tests in a single `mat3/` directory:

```javascript
mat3/
├── op-add.glsl              (mat3 + mat3 -> mat3, component-wise)
├── op-subtract.glsl         (mat3 - mat3 -> mat3, component-wise)
├── op-multiply.glsl         (mat3 * mat3 -> mat3, matrix multiplication)
├── op-multiply-vec.glsl     (mat3 * vec3 -> vec3, matrix-vector multiplication)
├── op-unary-minus.glsl      (-mat3 -> mat3, component-wise negation)
├── op-increment-pre.glsl    (++mat3 -> mat3, pre-increment)
├── op-increment-post.glsl   (mat3++ -> mat3, post-increment)
├── op-decrement-pre.glsl    (--mat3 -> mat3, pre-decrement)
├── op-decrement-post.glsl   (mat3-- -> mat3, post-decrement)
├── op-equal.glsl            (mat3 == mat3 -> bool, aggregate comparison)
├── op-not-equal.glsl        (mat3 != mat3 -> bool, aggregate comparison)
├── fn-transpose.glsl        (transpose(mat3) -> mat3)
├── fn-determinant.glsl      (determinant(mat3) -> float)
├── fn-inverse.glsl          (inverse(mat3) -> mat3)
├── fn-outer-product.glsl    (outerProduct(vec3, vec3) -> mat3)
├── fn-matrix-comp-mult.glsl (matrixCompMult(mat3, mat3) -> mat3)
├── from-scalar.glsl         (mat3(float) - diagonal matrix)
├── from-scalars.glsl        (mat3(float x9) - column-major)
├── from-vectors.glsl        (mat3(vec3, vec3, vec3) - column vectors)
├── from-matrix.glsl         (mat3(mat2), mat3(mat4) - padding/truncation)
├── from-mat.glsl            (mat3(mat3) - identity)
├── to-vec.glsl              (mat3 column extraction: mat3[0], mat3[1], mat3[2])
├── assign-simple.glsl       (mat3 = mat3)
├── assign-element.glsl      (mat3[0][0] = float - element assignment)
├── assign-column.glsl       (mat3[0] = vec3 - column assignment)
├── access-column.glsl       (mat3[0], mat3[1], mat3[2] - column access)
├── access-element.glsl      (mat3[0][0], mat3[1][1], mat3[2][2] - element access)
├── access-nested.glsl       (mat3[col][row] with computed indices)
├── ctrl-if.glsl             (if (mat3 == mat3) - control flow)
├── ctrl-while.glsl          (while (mat3 != mat3))
├── ctrl-for.glsl            (for (init; mat3 == mat3; update))
├── ctrl-do-while.glsl       (do { } while (mat3 != mat3))
├── ctrl-ternary.glsl        (mat3 == mat3 ? expr1 : expr2)
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

mat3 test_mat_operation_name() {
    // Test implementation
    return result;
    // Should be mat3(expected_col0_x, expected_col0_y, expected_col0_z, expected_col1_x, expected_col1_y, expected_col1_z, expected_col2_x, expected_col2_y, expected_col2_z)
}

// run: test_mat_operation_name() ~= mat3(expected_col0_x, expected_col0_y, expected_col0_z, expected_col1_x, expected_col1_y, expected_col1_z, expected_col2_x, expected_col2_y, expected_col2_z)
```

Note: Use `~=` for approximate equality due to floating-point precision issues. Matrix literals use column-major order.

## Key Test Categories

### 1. Arithmetic Operators

**op-add.glsl**: Test `+` operator (component-wise)
- `mat3 + mat3` → `mat3` (component-wise addition)
- Test with various matrices

**op-subtract.glsl**: Test `-` operator (component-wise)
- `mat3 - mat3` → `mat3` (component-wise subtraction)
- Test with various matrices

**op-multiply.glsl**: Test `*` operator (matrix multiplication)
- `mat3 * mat3` → `mat3` (matrix multiplication)
- Test matrix multiplication rules
- Test identity matrix properties

**op-multiply-vec.glsl**: Test matrix-vector multiplication
- `mat3 * vec3` → `vec3` (matrix-vector multiplication)
- Test transformation properties

### 2. Unary Operators

**op-unary-minus.glsl**: Test `-` unary operator (component-wise)
- `-mat3` → `mat3` (component-wise negation)
- Test with various matrices

### 3. Increment/Decrement Operators

**op-increment-pre.glsl**: Test `++` pre-increment
- `++mat3` → `mat3` (increment all elements, return new value)
- Must be on lvalue

**op-increment-post.glsl**: Test `++` post-increment
- `mat3++` → `mat3` (return old value, then increment all elements)
- Must be on lvalue

**op-decrement-pre.glsl**: Test `--` pre-decrement
- `--mat3` → `mat3` (decrement all elements, return new value)
- Must be on lvalue

**op-decrement-post.glsl**: Test `--` post-decrement
- `mat3--` → `mat3` (return old value, then decrement all elements)
- Must be on lvalue

### 4. Comparison Operators

**op-equal.glsl**: Test `==` operator (aggregate comparison)
- `mat3 == mat3` → `bool` (true if all elements equal)
- Test with identical matrices, different matrices

**op-not-equal.glsl**: Test `!=` operator (aggregate comparison)
- `mat3 != mat3` → `bool` (true if any element differs)

### 5. Built-in Functions

**fn-transpose.glsl**: Test `transpose()` built-in
- `transpose(mat3)` → `mat3` (transpose matrix)
- Test that transpose(transpose(m)) == m

**fn-determinant.glsl**: Test `determinant()` built-in
- `determinant(mat3)` → `float` (matrix determinant)
- Test determinant properties

**fn-inverse.glsl**: Test `inverse()` built-in
- `inverse(mat3)` → `mat3` (matrix inverse)
- Test that m * inverse(m) == identity (for invertible matrices)
- Test with singular matrices (undefined behavior)

**fn-outer-product.glsl**: Test `outerProduct()` built-in
- `outerProduct(vec3, vec3)` → `mat3` (outer product of vectors)
- Test that outer product creates expected matrix

**fn-matrix-comp-mult.glsl**: Test `matrixCompMult()` built-in
- `matrixCompMult(mat3, mat3)` → `mat3` (component-wise multiplication)
- Note: Different from matrix multiplication operator

### 6. Constructors

**from-scalar.glsl**: Test scalar constructor (diagonal matrix)
- `mat3(float)` - creates diagonal matrix with float on diagonal, 0 elsewhere
- `mat3(5.0)` → `mat3(5.0, 0.0, 0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 5.0)`

**from-scalars.glsl**: Test constructor from scalars (column-major)
- `mat3(float x9)` - 9 floats in column-major order
- `mat3(a, b, c, d, e, f, g, h, i)` → columns [a,d,g], [b,e,h], [c,f,i]

**from-vectors.glsl**: Test constructor from column vectors
- `mat3(vec3, vec3, vec3)` - three column vectors
- `mat3(vec3(a,b,c), vec3(d,e,f), vec3(g,h,i))` → expected matrix

**from-matrix.glsl**: Test constructor from other matrices
- `mat3(mat2)` - pads upper-left 2x2 with identity
- `mat3(mat4)` - truncates upper-left 3x3
- Test padding/truncation behavior

**from-mat.glsl**: Test identity constructor
- `mat3(mat3)` - identity constructor
- Should preserve all elements

### 7. Conversions

**to-vec.glsl**: Test column extraction
- `mat3[0]` → `vec3` (first column)
- `mat3[1]` → `vec3` (second column)
- `mat3[2]` → `vec3` (third column)
- Test column vector contents

### 8. Assignment

**assign-simple.glsl**: Test simple assignment
- `mat3 a = mat3(...); mat3 b = a;` - assignment
- Verify independence (modifying one doesn't affect the other)
- Self-assignment: `mat3 = mat3`

**assign-element.glsl**: Test element assignment
- `mat3[0][0] = float` - assign to specific element
- Test all elements: [0][0], [0][1], [0][2], [1][0], [1][1], [1][2], [2][0], [2][1], [2][2]
- Verify other elements unchanged

**assign-column.glsl**: Test column assignment
- `mat3[0] = vec3` - assign to entire column
- `mat3[1] = vec3` - assign to entire column
- `mat3[2] = vec3` - assign to entire column
- Test all three columns

### 9. Indexing and Access

**access-column.glsl**: Test column access
- `mat3[0]`, `mat3[1]`, `mat3[2]` - column access returns vec3
- Variable indexing: `mat3[i]` where `i` is computed
- Verify correct column vectors

**access-element.glsl**: Test element access
- `mat3[0][0]`, `mat3[0][1]`, `mat3[0][2]`, etc. - element access
- Verify correct scalar values

**access-nested.glsl**: Test nested/computed indexing
- `mat3[col][row]` with computed col/row indices
- Test bounds and correctness

### 10. Control Flow

**ctrl-if.glsl**: Test `if` statements with mat3
- `if (mat3 == mat3)` - condition using matrix comparison
- Test with equal/unequal matrices

**ctrl-while.glsl**: Test `while` loops with mat3
- `while (mat3 != mat3)` - loop condition

**ctrl-for.glsl**: Test `for` loops with mat3
- `for (init; mat3 == mat3; update)` - for loop condition

**ctrl-do-while.glsl**: Test `do-while` loops with mat3
- `do { } while (mat3 != mat3)` - do-while condition

**ctrl-ternary.glsl**: Test ternary operator with mat3
- `mat3 == mat3 ? expr1 : expr2` - ternary with mat3 condition

### 11. Edge Cases

**edge-identity.glsl**: Test identity matrix patterns
- `mat3(1.0)` - identity matrix
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

3. **Key Differences from mat2**:
   - 3x3 matrix (9 elements) vs 2x2 (4 elements)
   - Three columns instead of two
   - Constructor from mat2 (padding with identity)
   - Constructor from three vec3 columns
   - More complex indexing: [0], [1], [2]

4. **Expected Failures**: These tests are expected to fail initially, especially:
   - Matrix multiplication operators
   - Built-in functions (transpose, determinant, inverse, etc.)
   - Matrix constructors from scalars/vectors/matrices
   - Matrix indexing operations
   - Increment/decrement operators

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/matrix/arithmetic/addition.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/matrix/constructors/from-scalar.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/matrix/indexing/column-access.glsl`

## Files to Create

Create 25 test files in the flat `mat3/` directory structure above, with each file containing 3-10 test functions following the vec4/bool pattern. All files use the prefix naming convention:

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
