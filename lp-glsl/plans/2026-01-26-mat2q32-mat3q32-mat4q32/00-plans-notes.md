# Plan: Implement Mat2Q32, Mat3Q32, Mat4Q32

## Questions

### Q1: Which matrix types should we implement?

**Context:** GLSL supports Mat2, Mat3, and Mat4. The reference implementation only has Mat3. We need to decide if we should implement all three or just Mat3.

**Answer:** Implement all three (Mat2Q32, Mat3Q32, Mat4Q32) to match GLSL support and enable porting any GLSL code that uses matrices. This provides complete coverage.

### Q2: Should we use column-major or row-major storage?

**Context:** GLSL uses column-major storage for matrices. The reference implementation uses column-major storage. The compiler already expects column-major matrices.

**Answer:** Use column-major storage to match GLSL specification and the compiler's expectations. Storage layout: `[m00, m10, m20, m01, m11, m21, ...]` where `m[row][col]` represents element at row `row` and column `col`.

### Q3: Which operations should we implement?

**Context:** The reference Mat3 implementation includes:

- Construction: `new()`, `from_f32()`, `from_vec3()`, `identity()`, `zero()`
- Access: `get()`, `set()`, `col0()`, `col1()`, `col2()`
- Operations: `mul()` (matrix-matrix), `mul_vec3()` (matrix-vector), `transpose()`, `determinant()`, `inverse()`
- Operators: `Add`, `Sub`, `Mul<Mat3>`, `Mul<Vec3>`, `Mul<Fixed>`, `Div<Fixed>`, `Neg`

**Answer:** Implement all operations from the reference for Mat3Q32, and adapt them for Mat2Q32 and Mat4Q32. This includes:

- All construction methods
- Element access methods
- Matrix-matrix multiplication
- Matrix-vector multiplication (using Vec2Q32, Vec3Q32, Vec4Q32)
- Transpose
- Determinant
- Inverse (returns Option)
- All operator overloads

### Q4: Should matrix-vector multiplication use our Vec2Q32/Vec3Q32/Vec4Q32 types?

**Context:** We just implemented Vec2Q32, Vec3Q32, Vec4Q32. Matrix-vector multiplication should return these types.

**Answer:** Yes, use our vector types:

- `Mat2Q32 * Vec2Q32 -> Vec2Q32`
- `Mat3Q32 * Vec3Q32 -> Vec3Q32`
- `Mat4Q32 * Vec4Q32 -> Vec4Q32`

This provides a clean, integrated API.

### Q5: Should we implement from_vec constructors?

**Context:** The reference has `from_vec3()` which constructs a Mat3 from three Vec3 columns.

**Answer:** Yes, implement:

- `Mat2Q32::from_vec2(col0: Vec2Q32, col1: Vec2Q32) -> Mat2Q32`
- `Mat3Q32::from_vec3(col0: Vec3Q32, col1: Vec3Q32, col2: Vec3Q32) -> Mat3Q32`
- `Mat4Q32::from_vec4(col0: Vec4Q32, col1: Vec4Q32, col2: Vec4Q32, col3: Vec4Q32) -> Mat4Q32`

This matches the reference and provides a convenient way to construct matrices.

### Q6: Should we use fast Q32 operators or builtin functions?

**Context:** For vectors, we used fast Q32 operators directly (no saturation). Matrices involve many multiplications and additions.

**Answer:** Use fast Q32 operators directly, same as vectors. These are internal utilities optimized for performance. Matrix operations involve many operations, so fast operators are important.

### Q7: Should we implement all three matrices in separate files?

**Context:** The reference has Mat3 in a single file. We have three matrix types to implement.

**Answer:** Use separate files (`mat2_q32.rs`, `mat3_q32.rs`, `mat4_q32.rs`) following the same pattern as vectors. This keeps code organized and easier to navigate.

### Q8: Should we include comprehensive tests?

**Context:** The reference implementation has tests. We should have tests for all matrix operations.

**Answer:** Yes, include comprehensive tests similar to the reference implementation. Test:

- Construction methods
- Element access (`get()`, `set()`, column accessors)
- Matrix-matrix multiplication
- Matrix-vector multiplication
- Transpose
- Determinant
- Inverse (including singular matrix case)
- Operator overloads
- Edge cases (identity, zero, singular matrices)

### Q9: How should we handle inverse() for singular matrices?

**Context:** The reference implementation returns `Option<Mat3>` - `None` if determinant is zero (singular matrix).

**Answer:** Return `Option<MatXQ32>` - `None` if determinant is zero. This matches the reference and provides safe handling of singular matrices.

### Q10: Should matrices be no_std compatible?

**Context:** The crate uses `#![cfg_attr(not(feature = "std"), no_std)]` and all vector types are `no_std` compatible.

**Answer:** Yes, ensure all matrix types are `no_std` compatible. Use `core::ops` instead of `std::ops`. Tests can use `extern crate std` when needed.
