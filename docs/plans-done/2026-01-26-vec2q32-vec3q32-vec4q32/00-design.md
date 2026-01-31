# Design: Vec2Q32, Vec3Q32, Vec4Q32 Vector Types

## Overview

Implement vector types for Q32 fixed-point arithmetic to enable easy porting of GLSL code to Rust.
These types provide a clean, ergonomic API similar to GLSL vectors while using the fast Q32
fixed-point arithmetic.

## File Structure

```
lp-glsl/lp-glsl-builtins/src/util/
├── vec2_q32.rs              # NEW: Vec2Q32 type and implementation
├── vec3_q32.rs              # NEW: Vec3Q32 type and implementation
├── vec4_q32.rs              # NEW: Vec4Q32 type and implementation
├── mod.rs                   # UPDATE: Export new vector types
├── q32.rs                   # EXISTING: Q32 type (used by vectors)
└── test_helpers.rs          # EXISTING: Test utilities (used by vector tests)
```

## Types Summary

### Vec2Q32 (`util/vec2_q32.rs`)

```
Vec2Q32 - # NEW: 2D vector with Q32 components
├── x: Q32
└── y: Q32

Vec2Q32::new(x: Q32, y: Q32) -> Vec2Q32 - # NEW: Construct from components
Vec2Q32::from_f32(x: f32, y: f32) -> Vec2Q32 - # NEW: Construct from floats
Vec2Q32::from_i32(x: i32, y: i32) -> Vec2Q32 - # NEW: Construct from integers
Vec2Q32::zero() -> Vec2Q32 - # NEW: Zero vector
Vec2Q32::one() -> Vec2Q32 - # NEW: Vector with all components = 1

Vec2Q32::dot(self, rhs: Vec2Q32) -> Q32 - # NEW: Dot product
Vec2Q32::cross(self, rhs: Vec2Q32) -> Q32 - # NEW: Cross product (returns scalar)
Vec2Q32::length_squared(self) -> Q32 - # NEW: Length squared (avoids sqrt)
Vec2Q32::length(self) -> Q32 - # NEW: Length (magnitude)
Vec2Q32::distance(self, other: Vec2Q32) -> Q32 - # NEW: Distance to another vector
Vec2Q32::normalize(self) -> Vec2Q32 - # NEW: Normalize to unit vector

Vec2Q32::mul_comp(self, rhs: Vec2Q32) -> Vec2Q32 - # NEW: Component-wise multiply
Vec2Q32::div_comp(self, rhs: Vec2Q32) -> Vec2Q32 - # NEW: Component-wise divide

# Swizzle methods (GLSL-style)
Vec2Q32::x(self) -> Q32 - # NEW: Access x component
Vec2Q32::y(self) -> Q32 - # NEW: Access y component
Vec2Q32::r(self) -> Q32 - # NEW: Access r component (alias for x)
Vec2Q32::g(self) -> Q32 - # NEW: Access g component (alias for y)
Vec2Q32::s(self) -> Q32 - # NEW: Access s component (alias for x)
Vec2Q32::t(self) -> Q32 - # NEW: Access t component (alias for y)
Vec2Q32::xx(self) -> Vec2Q32 - # NEW: Swizzle (x, x)
Vec2Q32::xy(self) -> Vec2Q32 - # NEW: Swizzle (x, y) - identity
Vec2Q32::yx(self) -> Vec2Q32 - # NEW: Swizzle (y, x)
Vec2Q32::yy(self) -> Vec2Q32 - # NEW: Swizzle (y, y)
# ... RGBA and STPQ variants

Add<Vec2Q32> for Vec2Q32 - # NEW: Vector addition
Sub<Vec2Q32> for Vec2Q32 - # NEW: Vector subtraction
Mul<Q32> for Vec2Q32 - # NEW: Scalar multiplication
Div<Q32> for Vec2Q32 - # NEW: Scalar division
Neg for Vec2Q32 - # NEW: Negation
```

### Vec3Q32 (`util/vec3_q32.rs`)

```
Vec3Q32 - # NEW: 3D vector with Q32 components
├── x: Q32
├── y: Q32
└── z: Q32

Vec3Q32::new(x: Q32, y: Q32, z: Q32) -> Vec3Q32 - # NEW: Construct from components
Vec3Q32::from_f32(x: f32, y: f32, z: f32) -> Vec3Q32 - # NEW: Construct from floats
Vec3Q32::from_i32(x: i32, y: i32, z: i32) -> Vec3Q32 - # NEW: Construct from integers
Vec3Q32::zero() -> Vec3Q32 - # NEW: Zero vector
Vec3Q32::one() -> Vec3Q32 - # NEW: Vector with all components = 1

Vec3Q32::dot(self, rhs: Vec3Q32) -> Q32 - # NEW: Dot product
Vec3Q32::cross(self, rhs: Vec3Q32) -> Vec3Q32 - # NEW: Cross product (returns Vec3Q32)
Vec3Q32::length_squared(self) -> Q32 - # NEW: Length squared (avoids sqrt)
Vec3Q32::length(self) -> Q32 - # NEW: Length (magnitude)
Vec3Q32::distance(self, other: Vec3Q32) -> Q32 - # NEW: Distance to another vector
Vec3Q32::normalize(self) -> Vec3Q32 - # NEW: Normalize to unit vector
Vec3Q32::reflect(self, normal: Vec3Q32) -> Vec3Q32 - # NEW: Reflect vector around normal

Vec3Q32::mul_comp(self, rhs: Vec3Q32) -> Vec3Q32 - # NEW: Component-wise multiply
Vec3Q32::div_comp(self, rhs: Vec3Q32) -> Vec3Q32 - # NEW: Component-wise divide
Vec3Q32::clamp(self, min: Q32, max: Q32) -> Vec3Q32 - # NEW: Clamp components

# Swizzle methods (GLSL-style)
Vec3Q32::x(self) -> Q32 - # NEW: Access x component
Vec3Q32::y(self) -> Q32 - # NEW: Access y component
Vec3Q32::z(self) -> Q32 - # NEW: Access z component
Vec3Q32::r(self) -> Q32 - # NEW: Access r component (alias for x)
Vec3Q32::g(self) -> Q32 - # NEW: Access g component (alias for y)
Vec3Q32::b(self) -> Q32 - # NEW: Access b component (alias for z)
Vec3Q32::xy(self) -> Vec2Q32 - # NEW: Swizzle to Vec2Q32
Vec3Q32::xz(self) -> Vec2Q32 - # NEW: Swizzle to Vec2Q32
Vec3Q32::yz(self) -> Vec2Q32 - # NEW: Swizzle to Vec2Q32
Vec3Q32::xyz(self) -> Vec3Q32 - # NEW: Swizzle (identity)
Vec3Q32::xzy(self) -> Vec3Q32 - # NEW: Swizzle permutation
# ... More permutations and RGBA variants

Add<Vec3Q32> for Vec3Q32 - # NEW: Vector addition
Sub<Vec3Q32> for Vec3Q32 - # NEW: Vector subtraction
Mul<Q32> for Vec3Q32 - # NEW: Scalar multiplication
Div<Q32> for Vec3Q32 - # NEW: Scalar division
Neg for Vec3Q32 - # NEW: Negation
```

### Vec4Q32 (`util/vec4_q32.rs`)

```
Vec4Q32 - # NEW: 4D vector with Q32 components
├── x: Q32
├── y: Q32
├── z: Q32
└── w: Q32

Vec4Q32::new(x: Q32, y: Q32, z: Q32, w: Q32) -> Vec4Q32 - # NEW: Construct from components
Vec4Q32::from_f32(x: f32, y: f32, z: f32, w: f32) -> Vec4Q32 - # NEW: Construct from floats
Vec4Q32::from_i32(x: i32, y: i32, z: i32, w: i32) -> Vec4Q32 - # NEW: Construct from integers
Vec4Q32::zero() -> Vec4Q32 - # NEW: Zero vector
Vec4Q32::one() -> Vec4Q32 - # NEW: Vector with all components = 1

Vec4Q32::dot(self, rhs: Vec4Q32) -> Q32 - # NEW: Dot product
Vec4Q32::length_squared(self) -> Q32 - # NEW: Length squared (avoids sqrt)
Vec4Q32::length(self) -> Q32 - # NEW: Length (magnitude)
Vec4Q32::distance(self, other: Vec4Q32) -> Q32 - # NEW: Distance to another vector
Vec4Q32::normalize(self) -> Vec4Q32 - # NEW: Normalize to unit vector

Vec4Q32::mul_comp(self, rhs: Vec4Q32) -> Vec4Q32 - # NEW: Component-wise multiply
Vec4Q32::div_comp(self, rhs: Vec4Q32) -> Vec4Q32 - # NEW: Component-wise divide
Vec4Q32::clamp(self, min: Q32, max: Q32) -> Vec4Q32 - # NEW: Clamp components

# Swizzle methods (GLSL-style)
Vec4Q32::x(self) -> Q32 - # NEW: Access x component
Vec4Q32::y(self) -> Q32 - # NEW: Access y component
Vec4Q32::z(self) -> Q32 - # NEW: Access z component
Vec4Q32::w(self) -> Q32 - # NEW: Access w component
Vec4Q32::r(self) -> Q32 - # NEW: Access r component (alias for x)
Vec4Q32::g(self) -> Q32 - # NEW: Access g component (alias for y)
Vec4Q32::b(self) -> Q32 - # NEW: Access b component (alias for z)
Vec4Q32::a(self) -> Q32 - # NEW: Access a component (alias for w)
Vec4Q32::xy(self) -> Vec2Q32 - # NEW: Swizzle to Vec2Q32
Vec4Q32::xyz(self) -> Vec3Q32 - # NEW: Swizzle to Vec3Q32
Vec4Q32::xyzw(self) -> Vec4Q32 - # NEW: Swizzle (identity)
Vec4Q32::rgba(self) -> Vec4Q32 - # NEW: Swizzle (identity, RGBA variant)
# ... More swizzle combinations

Add<Vec4Q32> for Vec4Q32 - # NEW: Vector addition
Sub<Vec4Q32> for Vec4Q32 - # NEW: Vector subtraction
Mul<Q32> for Vec4Q32 - # NEW: Scalar multiplication
Div<Q32> for Vec4Q32 - # NEW: Scalar division
Neg for Vec4Q32 - # NEW: Negation
```

## Design Decisions

### 1. Use Q32 Type for Components

Vector types use `Q32` wrapper type for components. This provides:

- Type safety
- Clean API similar to reference implementation
- Zero runtime overhead (newtype wrapper is optimized away)
- Easy to use in Rust code ported from GLSL

### 2. Fast Arithmetic Operations

All arithmetic operations use Q32's fast operators directly (no saturation):

- `Vec2Q32 + Vec2Q32` uses `Q32::add` for each component
- `Vec2Q32 * Q32` uses `Q32::mul` for each component
- `Vec2Q32 / Q32` uses `Q32::div` for each component

These are internal utilities optimized for performance. If saturation is needed, users can use the
builtin functions directly.

### 3. Length Calculation Uses Builtin Sqrt

While vector operations use fast Q32 operators, `length()` uses `__lp_q32_sqrt` from builtins since
we need a sqrt function. This is the only builtin function used by the vector types.

### 4. GLSL-Style Swizzle Methods

Extensive swizzle methods are provided to match GLSL behavior:

- Component accessors: `.x()`, `.y()`, `.z()`, `.w()`
- Color accessors: `.r()`, `.g()`, `.b()`, `.a()`
- Texture accessors: `.s()`, `.t()`, `.p()`, `.q()`
- 2-component swizzles returning `Vec2Q32`
- 3-component swizzles returning `Vec3Q32` (for vec3/vec4)
- 4-component swizzles returning `Vec4Q32` (for vec4)

This makes porting GLSL code straightforward.

### 5. Cross Product Behavior

Matches GLSL:

- `Vec2Q32::cross()` returns `Q32` (scalar, z-component of 3D cross product)
- `Vec3Q32::cross()` returns `Vec3Q32` (3D cross product)

### 6. Reflect Method

`Vec3Q32::reflect()` is implemented for lighting calculations. Formula: `v - 2 * dot(v, n) * n`

### 7. Normalization Edge Case

When normalizing a zero vector (length = 0), return a zero vector rather than panicking. This
matches GLSL behavior and the reference implementation.

### 8. no_std Compatibility

All vector types are `no_std` compatible:

- Use `core::ops` instead of `std::ops`
- Tests can use `extern crate std` when needed (like q32 builtin tests)

### 9. Separate Files

Each vector type is in its own file (`vec2_q32.rs`, `vec3_q32.rs`, `vec4_q32.rs`) for organization
and easier navigation.

### 10. Module Exports

Vector types are exported from `util/mod.rs` only. They can be re-exported at crate root later if
needed.

## Implementation Notes

### Reference Implementation

The reference implementation at `/Users/yona/dev/photomancer/lpmini2024/crates/lp-math/src/fixed/`
provides the structure and API to follow. Key differences:

- Reference uses `Fixed` type, we use `Q32`
- Reference uses `Fixed` operators, we use `Q32` operators (both are fast)
- Reference uses `advanced::sqrt`, we use `__lp_q32_sqrt` builtin

### Testing Strategy

Comprehensive tests similar to reference implementation:

- Use `test_helpers` module for conversion utilities
- Test construction methods
- Test basic arithmetic operations
- Test dot product, cross product
- Test length and normalization
- Test distance calculations
- Test component-wise operations
- Test swizzle methods
- Test edge cases (zero vectors, normalization of zero vectors, etc.)

### Performance Considerations

- All operations use fast Q32 operators (no saturation checks)
- Inline all methods with `#[inline(always)]`
- Swizzle methods are simple field access/construction (zero cost)
- Length calculation uses builtin sqrt (only builtin function used)

## Integration Points

### Module Structure

Vector types are added to `lp-glsl-builtins/src/util/`:

- `vec2_q32.rs` - Vec2Q32 implementation
- `vec3_q32.rs` - Vec3Q32 implementation
- `vec4_q32.rs` - Vec4Q32 implementation
- `mod.rs` - Export all vector types

### Dependencies

- `util/q32.rs` - Q32 type and operators
- `builtins/q32/sqrt.rs` - For `length()` method
- `util/test_helpers.rs` - For test utilities

### Usage Example

```rust
use lp_glsl_builtins::util::{Vec2Q32, Vec3Q32, Q32};

// Create vectors
let v2 = Vec2Q32::from_f32(1.0, 2.0);
let v3 = Vec3Q32::new(Q32::from_i32(1), Q32::from_i32(2), Q32::from_i32(3));

// Arithmetic
let sum = v2 + Vec2Q32::from_f32(3.0, 4.0);
let scaled = v2 * Q32::from_f32(2.0);

// Math operations
let dot = v2.dot(Vec2Q32::from_f32(5.0, 6.0));
let len = v2.length();
let normalized = v2.normalize();

// Swizzles
let xy = v3.xy(); // Vec2Q32
let xyz = v3.xyz(); // Vec3Q32 (identity)
```
