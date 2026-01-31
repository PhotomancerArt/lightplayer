# Design: Mat2Q32, Mat3Q32, Mat4Q32 Matrix Types

## Overview

Implement matrix types for Q32 fixed-point arithmetic to enable easy porting of GLSL code to Rust.
These types provide a clean, ergonomic API similar to GLSL matrices while using fast Q32 fixed-point
arithmetic.

## File Structure

```
lp-glsl/lp-glsl-builtins/src/util/
├── mat2_q32.rs              # NEW: Mat2Q32 type and implementation
├── mat3_q32.rs              # NEW: Mat3Q32 type and implementation
├── mat4_q32.rs              # NEW: Mat4Q32 type and implementation
├── mod.rs                   # UPDATE: Export new matrix types
├── q32.rs                   # EXISTING: Q32 type (used by matrices)
├── vec2_q32.rs              # EXISTING: Vec2Q32 (used by Mat2Q32)
├── vec3_q32.rs              # EXISTING: Vec3Q32 (used by Mat3Q32)
└── vec4_q32.rs              # EXISTING: Vec4Q32 (used by Mat4Q32)
```

## Types Summary

### Mat2Q32 (`util/mat2_q32.rs`)

```
Mat2Q32 - # NEW: 2x2 matrix with Q32 components (column-major storage)
├── m: [Q32; 4]              # Column-major: [m00, m10, m01, m11]

Mat2Q32::new(m00, m10, m01, m11) -> Mat2Q32 - # NEW: Construct from components (column-major)
Mat2Q32::from_f32(...) -> Mat2Q32 - # NEW: Construct from floats
Mat2Q32::from_vec2(col0, col1) -> Mat2Q32 - # NEW: Construct from Vec2Q32 columns
Mat2Q32::identity() -> Mat2Q32 - # NEW: Identity matrix
Mat2Q32::zero() -> Mat2Q32 - # NEW: Zero matrix

Mat2Q32::get(row, col) -> Q32 - # NEW: Get element at row, col
Mat2Q32::set(&mut self, row, col, value) - # NEW: Set element at row, col
Mat2Q32::col0() -> Vec2Q32 - # NEW: Get column 0
Mat2Q32::col1() -> Vec2Q32 - # NEW: Get column 1

Mat2Q32::mul(self, rhs: Mat2Q32) -> Mat2Q32 - # NEW: Matrix-matrix multiplication
Mat2Q32::mul_vec2(self, v: Vec2Q32) -> Vec2Q32 - # NEW: Matrix-vector multiplication
Mat2Q32::transpose(self) -> Mat2Q32 - # NEW: Transpose matrix
Mat2Q32::determinant(self) -> Q32 - # NEW: Calculate determinant
Mat2Q32::inverse(self) -> Option<Mat2Q32> - # NEW: Calculate inverse (None if singular)

Add<Mat2Q32> for Mat2Q32 - # NEW: Matrix addition
Sub<Mat2Q32> for Mat2Q32 - # NEW: Matrix subtraction
Mul<Mat2Q32> for Mat2Q32 - # NEW: Matrix-matrix multiplication
Mul<Vec2Q32> for Mat2Q32 - # NEW: Matrix-vector multiplication
Mul<Q32> for Mat2Q32 - # NEW: Scalar multiplication
Div<Q32> for Mat2Q32 - # NEW: Scalar division
Neg for Mat2Q32 - # NEW: Negation
```

### Mat3Q32 (`util/mat3_q32.rs`)

```
Mat3Q32 - # NEW: 3x3 matrix with Q32 components (column-major storage)
├── m: [Q32; 9]              # Column-major: [m00, m10, m20, m01, m11, m21, m02, m12, m22]

Mat3Q32::new(m00, m10, m20, m01, m11, m21, m02, m12, m22) -> Mat3Q32 - # NEW: Construct from components
Mat3Q32::from_f32(...) -> Mat3Q32 - # NEW: Construct from floats
Mat3Q32::from_vec3(col0, col1, col2) -> Mat3Q32 - # NEW: Construct from Vec3Q32 columns
Mat3Q32::identity() -> Mat3Q32 - # NEW: Identity matrix
Mat3Q32::zero() -> Mat3Q32 - # NEW: Zero matrix

Mat3Q32::get(row, col) -> Q32 - # NEW: Get element at row, col
Mat3Q32::set(&mut self, row, col, value) - # NEW: Set element at row, col
Mat3Q32::col0() -> Vec3Q32 - # NEW: Get column 0
Mat3Q32::col1() -> Vec3Q32 - # NEW: Get column 1
Mat3Q32::col2() -> Vec3Q32 - # NEW: Get column 2

Mat3Q32::mul(self, rhs: Mat3Q32) -> Mat3Q32 - # NEW: Matrix-matrix multiplication
Mat3Q32::mul_vec3(self, v: Vec3Q32) -> Vec3Q32 - # NEW: Matrix-vector multiplication
Mat3Q32::transpose(self) -> Mat3Q32 - # NEW: Transpose matrix
Mat3Q32::determinant(self) -> Q32 - # NEW: Calculate determinant (Sarrus' rule)
Mat3Q32::inverse(self) -> Option<Mat3Q32> - # NEW: Calculate inverse (None if singular)

Add<Mat3Q32> for Mat3Q32 - # NEW: Matrix addition
Sub<Mat3Q32> for Mat3Q32 - # NEW: Matrix subtraction
Mul<Mat3Q32> for Mat3Q32 - # NEW: Matrix-matrix multiplication
Mul<Vec3Q32> for Mat3Q32 - # NEW: Matrix-vector multiplication
Mul<Q32> for Mat3Q32 - # NEW: Scalar multiplication
Div<Q32> for Mat3Q32 - # NEW: Scalar division
Neg for Mat3Q32 - # NEW: Negation
```

### Mat4Q32 (`util/mat4_q32.rs`)

```
Mat4Q32 - # NEW: 4x4 matrix with Q32 components (column-major storage)
├── m: [Q32; 16]             # Column-major: [m00, m10, m20, m30, m01, m11, m21, m31, ...]

Mat4Q32::new(...) -> Mat4Q32 - # NEW: Construct from 16 components (column-major)
Mat4Q32::from_f32(...) -> Mat4Q32 - # NEW: Construct from floats
Mat4Q32::from_vec4(col0, col1, col2, col3) -> Mat4Q32 - # NEW: Construct from Vec4Q32 columns
Mat4Q32::identity() -> Mat4Q32 - # NEW: Identity matrix
Mat4Q32::zero() -> Mat4Q32 - # NEW: Zero matrix

Mat4Q32::get(row, col) -> Q32 - # NEW: Get element at row, col
Mat4Q32::set(&mut self, row, col, value) - # NEW: Set element at row, col
Mat4Q32::col0() -> Vec4Q32 - # NEW: Get column 0
Mat4Q32::col1() -> Vec4Q32 - # NEW: Get column 1
Mat4Q32::col2() -> Vec4Q32 - # NEW: Get column 2
Mat4Q32::col3() -> Vec4Q32 - # NEW: Get column 3

Mat4Q32::mul(self, rhs: Mat4Q32) -> Mat4Q32 - # NEW: Matrix-matrix multiplication
Mat4Q32::mul_vec4(self, v: Vec4Q32) -> Vec4Q32 - # NEW: Matrix-vector multiplication
Mat4Q32::transpose(self) -> Mat4Q32 - # NEW: Transpose matrix
Mat4Q32::determinant(self) -> Q32 - # NEW: Calculate determinant (Laplace expansion)
Mat4Q32::inverse(self) -> Option<Mat4Q32> - # NEW: Calculate inverse (None if singular)

Add<Mat4Q32> for Mat4Q32 - # NEW: Matrix addition
Sub<Mat4Q32> for Mat4Q32 - # NEW: Matrix subtraction
Mul<Mat4Q32> for Mat4Q32 - # NEW: Matrix-matrix multiplication
Mul<Vec4Q32> for Mat4Q32 - # NEW: Matrix-vector multiplication
Mul<Q32> for Mat4Q32 - # NEW: Scalar multiplication
Div<Q32> for Mat4Q32 - # NEW: Scalar division
Neg for Mat4Q32 - # NEW: Negation
```

## Design Decisions

### 1. Column-Major Storage

Matrices use column-major storage to match GLSL specification:

- Storage: `[m00, m10, m20, m01, m11, m21, ...]` (column-major)
- Access: `m[col * rows + row]` for element at `row`, `col`
- Mat2Q32: `[m00, m10, m01, m11]` (4 elements)
- Mat3Q32: `[m00, m10, m20, m01, m11, m21, m02, m12, m22]` (9 elements)
- Mat4Q32: `[m00, m10, m20, m30, m01, m11, m21, m31, m02, m12, m22, m32, m03, m13, m23, m33]` (16
  elements)

### 2. Fast Q32 Operators

All operations use Q32's fast operators directly (no saturation):

- Matrix-matrix multiplication uses `Q32::mul` and `Q32::add`
- Matrix-vector multiplication uses `Q32::mul` and `Q32::add`
- Determinant and inverse calculations use fast Q32 operators

### 3. Integration with Vector Types

Matrix-vector multiplication uses our vector types:

- `Mat2Q32 * Vec2Q32 -> Vec2Q32`
- `Mat3Q32 * Vec3Q32 -> Vec3Q32`
- `Mat4Q32 * Vec4Q32 -> Vec4Q32`

This provides a clean, integrated API.

### 4. Determinant Algorithms

- Mat2Q32: Simple 2x2 determinant: `m00*m11 - m01*m10`
- Mat3Q32: Sarrus' rule (as in reference)
- Mat4Q32: Laplace expansion (cofactor expansion)

### 5. Inverse Calculation

- All matrix types return `Option<MatXQ32>` for `inverse()`
- Returns `None` if determinant is zero (singular matrix)
- Uses cofactor matrix and adjugate (transpose of cofactor) divided by determinant

### 6. no_std Compatibility

All matrix types are `no_std` compatible:

- Use `core::ops` instead of `std::ops`
- Tests can use `extern crate std` when needed

### 7. Separate Files

Each matrix type is in its own file (`mat2_q32.rs`, `mat3_q32.rs`, `mat4_q32.rs`) for organization
and easier navigation.

### 8. Module Exports

Matrix types are exported from `util/mod.rs` only. They can be re-exported at crate root later if
needed.

## Implementation Notes

### Reference Implementation

The reference implementation at
`/Users/yona/dev/photomancer/lpmini2024/crates/lp-math/src/fixed/mat3.rs` provides the structure and
API to follow. Key differences:

- Reference uses `Fixed` type, we use `Q32`
- Reference uses `Fixed` operators, we use `Q32` operators (both are fast)
- Reference has Mat3 only, we implement Mat2, Mat3, and Mat4

### Matrix Multiplication

Matrix-matrix multiplication follows standard algorithm:

- For each element in result: sum of products of row from first matrix and column from second matrix
- Column-major storage means careful indexing: `m[col * rows + row]`

### Determinant Calculation

- Mat2Q32: `det = m00*m11 - m01*m10`
- Mat3Q32: Sarrus' rule (reference implementation)
- Mat4Q32: Laplace expansion (recursive using 3x3 determinants)

### Inverse Calculation

Uses standard algorithm:

1. Calculate determinant
2. If determinant is zero, return `None`
3. Calculate cofactor matrix
4. Transpose cofactor matrix (adjugate)
5. Divide adjugate by determinant

### Testing Strategy

Comprehensive tests similar to reference implementation:

- Use `test_helpers` module for conversion utilities
- Test construction methods
- Test element access (`get()`, `set()`, column accessors)
- Test matrix-matrix multiplication
- Test matrix-vector multiplication
- Test transpose
- Test determinant
- Test inverse (including singular matrix case)
- Test operator overloads
- Test edge cases (identity, zero, singular matrices)

### Performance Considerations

- All operations use fast Q32 operators (no saturation checks)
- Inline all methods with `#[inline(always)]`
- Matrix operations involve many multiplications - fast operators are critical
- Determinant and inverse are computationally expensive but necessary

## Integration Points

### Module Structure

Matrix types are added to `lp-glsl-builtins/src/util/`:

- `mat2_q32.rs` - Mat2Q32 implementation
- `mat3_q32.rs` - Mat3Q32 implementation
- `mat4_q32.rs` - Mat4Q32 implementation
- `mod.rs` - Export all matrix types

### Dependencies

- `util/q32.rs` - Q32 type and operators
- `util/vec2_q32.rs` - Vec2Q32 (for Mat2Q32)
- `util/vec3_q32.rs` - Vec3Q32 (for Mat3Q32)
- `util/vec4_q32.rs` - Vec4Q32 (for Mat4Q32)
- `util/test_helpers.rs` - For test utilities

### Usage Example

```rust
use lp_glsl_builtins::util::{Mat2Q32, Mat3Q32, Vec2Q32, Vec3Q32, Q32};

// Create matrices
let m2 = Mat2Q32::identity();
let m3 = Mat3Q32::from_f32(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);

// Matrix operations
let m2_t = m2.transpose();
let det = m2.determinant();
let inv = m2.inverse().unwrap();

// Matrix-vector multiplication
let v2 = Vec2Q32::from_f32(1.0, 2.0);
let result = m2 * v2;

// Matrix-matrix multiplication
let m2_product = m2 * m2;
```
