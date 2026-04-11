# Phase 2: Layout Computation (std430)

## Scope

Implement size() and alignment() methods for all GlslType variants under std430 rules.

## Implementation Details

### 1. Create layout.rs module (lpir/src/layout.rs)

```rust
//! Memory layout computation for GLSL types.
//! Currently implements std430 rules only.

use crate::glsl_metadata::{GlslType, StructMember, LayoutRules, ScalarType};

/// Compute size of a type under given layout rules.
pub fn type_size(ty: &GlslType, rules: LayoutRules) -> usize {
    match rules {
        LayoutRules::Std430 => std430_size(ty),
        LayoutRules::Std140 => panic!("std140 not yet implemented"),
    }
}

/// Compute alignment of a type under given layout rules.
pub fn type_alignment(ty: &GlslType, rules: LayoutRules) -> usize {
    match rules {
        LayoutRules::Std430 => std430_alignment(ty),
        LayoutRules::Std140 => panic!("std140 not yet implemented"),
    }
}

/// Round up to alignment.
pub fn round_up(size: usize, alignment: usize) -> usize {
    ((size + alignment - 1) / alignment) * alignment
}

// std430 implementation
fn std430_size(ty: &GlslType) -> usize {
    use GlslType::*;
    match ty {
        // Scalars: 4 bytes
        Float | Int | UInt | Bool => 4,
        
        // Vectors
        Vec2 | IVec2 | UVec2 | BVec2 => 8,
        Vec3 | IVec3 | UVec3 | BVec3 => 12,  // NOT padded to 16!
        Vec4 | IVec4 | UVec4 | BVec4 => 16,
        
        // Matrices: columns * vec size, column-major
        Mat2 => 2 * 8,   // 2 vec2 columns
        Mat3 => 3 * 12,  // 3 vec3 columns
        Mat4 => 4 * 16,  // 4 vec4 columns
        
        // Arrays: stride * length
        Array { element, len } => {
            let stride = std430_array_stride(element);
            stride * (*len as usize)
        }
        
        // Structs: sum of member sizes, rounded to struct alignment
        Struct { members, .. } => {
            let mut offset = 0;
            let mut max_align = 1;
            for member in members {
                let align = std430_alignment(&member.ty);
                let size = std430_size(&member.ty);
                offset = round_up(offset, align) + size;
                max_align = max_align.max(align);
            }
            round_up(offset, max_align)
        }
        
        Void => 0,
    }
}

fn std430_alignment(ty: &GlslType) -> usize {
    use GlslType::*;
    match ty {
        // Scalars: 4 bytes
        Float | Int | UInt | Bool => 4,
        
        // Vectors: component size * component count
        Vec2 | IVec2 | UVec2 | BVec2 => 8,
        Vec3 | IVec3 | UVec3 | BVec3 => 4,  // vec3 aligns to 4!
        Vec4 | IVec4 | UVec4 | BVec4 => 16,
        
        // Matrices: alignment of column vector
        Mat2 => 8,   // vec2 alignment
        Mat3 => 4,   // vec3 alignment
        Mat4 => 16,  // vec4 alignment
        
        // Arrays: alignment of element
        Array { element, .. } => std430_alignment(element),
        
        // Structs: max member alignment
        Struct { members, .. } => {
            members.iter()
                .map(|m| std430_alignment(&m.ty))
                .max()
                .unwrap_or(1)
        }
        
        Void => 1,
    }
}

fn std430_array_stride(element: &GlslType) -> usize {
    // Array stride = element size (no rounding in std430)
    let size = std430_size(element);
    let align = std430_alignment(element);
    round_up(size, align)
}
```

### 2. Add methods to GlslType

In glsl_metadata.rs:
```rust
impl GlslType {
    /// Compute size under given layout rules.
    pub fn size(&self, rules: LayoutRules) -> usize {
        crate::layout::type_size(self, rules)
    }
    
    /// Compute alignment under given layout rules.
    pub fn alignment(&self, rules: LayoutRules) -> usize {
        crate::layout::type_alignment(self, rules)
    }
}
```

## Code Organization

- Place helper functions (std430_size, std430_alignment) at the bottom
- Place public API (type_size, type_alignment, round_up) at the top
- Document each std430 rule in comments

## Tests

```rust
#[test]
fn std430_scalar_sizes() {
    assert_eq!(GlslType::Float.size(LayoutRules::Std430), 4);
    assert_eq!(GlslType::Int.size(LayoutRules::Std430), 4);
}

#[test]
fn std430_vec3_is_not_padded() {
    // Critical std430 rule: vec3 is 12 bytes, not 16
    assert_eq!(GlslType::Vec3.size(LayoutRules::Std430), 12);
    assert_eq!(GlslType::Vec3.alignment(LayoutRules::Std430), 4);
}

#[test]
fn std430_vec4_is_16_bytes() {
    assert_eq!(GlslType::Vec4.size(LayoutRules::Std430), 16);
    assert_eq!(GlslType::Vec4.alignment(LayoutRules::Std430), 16);
}

#[test]
fn std430_array_stride() {
    // Array of vec3: stride = 12 (rounded to alignment 4 = 12)
    let arr = GlslType::Array {
        element: Box::new(GlslType::Vec3),
        len: 4,
    };
    assert_eq!(arr.size(LayoutRules::Std430), 48);  // 12 * 4
}

#[test]
fn std430_simple_struct() {
    let s = GlslType::Struct {
        name: Some("Test".to_string()),
        members: vec![
            StructMember { name: Some("a".to_string()), ty: GlslType::Float },
            StructMember { name: Some("b".to_string()), ty: GlslType::Vec3 },
        ],
    };
    // Float: 4 bytes at offset 0
    // Vec3: needs 4-byte alignment, starts at 4, size 12
    // Total: 16, struct alignment = max(4, 4) = 4
    // Size = round_up(16, 4) = 16
    assert_eq!(s.size(LayoutRules::Std430), 16);
}
```

## Validation

```bash
cargo check -p lpir
cargo test -p lpir layout::
```

## Notes

- The vec3 handling (12 bytes, 4-byte aligned) is the critical difference from std140
- Document this heavily - it's a common source of confusion
- round_up is a utility that belongs at the bottom of the file