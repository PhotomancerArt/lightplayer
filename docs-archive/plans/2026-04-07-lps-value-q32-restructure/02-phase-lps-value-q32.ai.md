# Phase 2: Complete lps_value_q32.rs

## Scope

Complete the `LpsValueQ32` enum with derives, methods, and conversion functions to/from `LpsValueF32`. This is the core type for Q32 semantic representation.

## Code Organization

- Type definition with derives first
- Helper methods (eq, approx_eq) after type
- Conversion functions `lps_value_to_q32()` and `q32_to_lps_value()` at bottom

## Implementation

### Type Definition

```rust
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use lps_q32::Q32;
use lps_shared::{LpsType, LpsValueF32};

/// Q32 semantic representation of shader values
///
/// Mirrors LpsValueF32 but uses Q32 for float components,
/// representing the exact fixed-point values.
#[derive(Clone, Debug, PartialEq)]
pub enum LpsValueQ32 {
    I32(i32),
    U32(u32),
    F32(Q32),
    Bool(bool),
    Vec2([Q32; 2]),
    Vec3([Q32; 3]),
    Vec4([Q32; 4]),
    IVec2([i32; 2]),
    IVec3([i32; 3]),
    IVec4([i32; 4]),
    UVec2([u32; 2]),
    UVec3([u32; 3]),
    UVec4([u32; 4]),
    BVec2([bool; 2]),
    BVec3([bool; 3]),
    BVec4([bool; 4]),
    Mat2x2([[Q32; 2]; 2]), // Column-major: [col0, col1]
    Mat3x3([[Q32; 3]; 3]),
    Mat4x4([[Q32; 4]; 4]),
    Array(Box<[LpsValueQ32]>),
    Struct {
        name: Option<String>,
        fields: Vec<(String, LpsValueQ32)>,
    },
}
```

### Equality and Approximate Equality

Q32 equality is exact (raw bits match). For approximate float comparison:

```rust
impl LpsValueQ32 {
    /// Exact equality - Q32 values match by raw bits
    pub fn eq(&self, other: &Self) -> bool {
        // Use derived PartialEq - exact comparison
        self == other
    }

    /// Approximate equality for floats with Q32 tolerance
    ///
    /// For non-float types, falls back to exact equality.
    pub fn approx_eq(&self, other: &Self, tolerance: Q32) -> bool {
        // Component-wise comparison with Q32 tolerance for float types
        // Ints/bools use exact comparison
        todo!("Implement component-wise approx_eq")
    }

    pub const DEFAULT_TOLERANCE: Q32 = Q32::from_fixed(655); // ~0.01 in Q32

    pub fn approx_eq_default(&self, other: &Self) -> bool {
        self.approx_eq(other, Self::DEFAULT_TOLERANCE)
    }
}
```

### Conversion Functions

```rust
/// Convert LpsValueF32 to LpsValueQ32
///
/// Uses saturating conversion for floats (Q32::from_f32_saturating)
pub fn lps_value_to_q32(ty: &LpsType, v: &LpsValueF32) -> Result<LpsValueQ32, String> {
    match (ty, v) {
        (LpsType::Float, LpsValueF32::F32(f)) => {
            Ok(LpsValueQ32::F32(Q32::from_f32_saturating(*f)))
        }
        (LpsType::Int, LpsValueF32::I32(i)) => Ok(LpsValueQ32::I32(*i)),
        (LpsType::UInt, LpsValueF32::U32(u)) => Ok(LpsValueQ32::U32(*u)),
        (LpsType::Bool, LpsValueF32::Bool(b)) => Ok(LpsValueQ32::Bool(*b)),
        // Vec2, Vec3, Vec4: component-wise Q32 conversion
        // IVec*, UVec*, BVec*: pass through
        // Mat*: column-major matrix conversion
        // Array, Struct: recursive
        _ => Err(format!("type mismatch: {ty:?} vs {v:?}")),
    }
}

/// Convert LpsValueQ32 to LpsValueF32
///
/// Exact conversion for all types (Q32::to_f32 for floats)
pub fn q32_to_lps_value(ty: &LpsType, v: LpsValueQ32) -> Result<LpsValueF32, String> {
    match (ty, v) {
        (LpsType::Float, LpsValueQ32::F32(q)) => Ok(LpsValueF32::F32(q.to_f32())),
        (LpsType::Int, LpsValueQ32::I32(i)) => Ok(LpsValueF32::I32(i)),
        (LpsType::UInt, LpsValueQ32::U32(u)) => Ok(LpsValueF32::U32(u)),
        (LpsType::Bool, LpsValueQ32::Bool(b)) => Ok(LpsValueF32::Bool(b)),
        // Component-wise for vectors, matrices
        // Recursive for arrays, structs
        _ => Err(format!("type mismatch: {ty:?} vs {v:?}")),
    }
}
```

## Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_scalar() {
        let f32_val = LpsValueF32::F32(1.5);
        let q32_val = lps_value_to_q32(&LpsType::Float, &f32_val).unwrap();
        let back = q32_to_lps_value(&LpsType::Float, q32_val).unwrap();
        assert!((back.approx_eq_default(&f32_val)));
    }

    #[test]
    fn saturates_out_of_range() {
        let f32_val = LpsValueF32::F32(50000.0);
        let q32_val = lps_value_to_q32(&LpsType::Float, &f32_val).unwrap();
        // Should be at max Q32 value
        match q32_val {
            LpsValueQ32::F32(q) => assert_eq!(q.to_fixed(), 0x7FFF_FFFF),
            _ => panic!("expected F32"),
        }
    }
}
```

## Validate

```bash
cargo check -p lps-shared
cargo test -p lps-shared lps_value_q32
```
