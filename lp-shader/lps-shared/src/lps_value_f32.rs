//! GLSL value types for function arguments and return values

use alloc::boxed::Box;

use crate::texture_format::LpsTexture2DDescriptor;

/// Shader value as rust enum, heap-allocated
///
/// ## Matrix Storage Format
///
/// Matrices are stored in **column-major order** per GLSL specification.
/// The internal representation uses `m[col][row]` indexing, matching GLSL semantics.
///
/// Example: `mat2(vec2(1.0, 2.0), vec2(3.0, 4.0))`
/// - Column 0: [1.0, 2.0]
/// - Column 1: [3.0, 4.0]
/// - Storage (column-major): [1.0, 2.0, 3.0, 4.0]
/// - Internal representation: `[[1.0, 2.0], [3.0, 4.0]]` (m[col][row])
///   - m[0][0] = 1.0 (col 0, row 0)
///   - m[0][1] = 2.0 (col 0, row 1)
///   - m[1][0] = 3.0 (col 1, row 0)
///   - m[1][1] = 4.0 (col 1, row 1)
///
/// To access column `col`, use `m[col][row]` for `row` in 0..rows.
/// To access row `row`, use `m[col][row]` for `col` in 0..cols.
#[derive(Debug, Clone)]
pub enum LpsValueF32 {
    I32(i32),
    U32(u32),
    F32(f32),
    Bool(bool),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    IVec2([i32; 2]),
    IVec3([i32; 3]),
    IVec4([i32; 4]),
    UVec2([u32; 2]),
    UVec3([u32; 3]),
    UVec4([u32; 4]),
    BVec2([bool; 2]),
    BVec3([bool; 3]),
    BVec4([bool; 4]),
    Mat2x2([[f32; 2]; 2]), // [[col0_row0, col0_row1], [col1_row0, col1_row1]]
    Mat3x3([[f32; 3]; 3]), // [[col0_row0, col0_row1, col0_row2], [col1_row0, ...], ...]
    Mat4x4([[f32; 4]; 4]), // [[col0_row0, col0_row1, col0_row2, col0_row3], [col1_row0, ...], ...]
    /// Fixed-size array; elements use the same recursive shape (scalars, vectors, matrices, nested arrays).
    Array(Box<[LpsValueF32]>),
    /// Struct instance; `fields` are in declaration order (names match [`StructMember::name`] when present).
    Struct {
        name: Option<alloc::string::String>,
        fields: alloc::vec::Vec<(alloc::string::String, LpsValueF32)>,
    },
    /// 2D texture reference (`LpsType::Texture2D`), as an opaque std430 four-`u32` descriptor.
    Texture2D(LpsTexture2DDescriptor),
}

impl LpsValueF32 {
    /// Exact equality comparison (==)
    /// For integers and booleans: exact match required
    /// For floats: exact match required (use `approx_eq` for tolerance-based comparison)
    /// For vectors/matrices: exact match for all components
    pub fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (LpsValueF32::I32(a), LpsValueF32::I32(b)) => a == b,
            (LpsValueF32::U32(a), LpsValueF32::U32(b)) => a == b,
            (LpsValueF32::F32(a), LpsValueF32::F32(b)) => a == b, // Exact equality
            (LpsValueF32::Bool(a), LpsValueF32::Bool(b)) => a == b,
            (LpsValueF32::Vec2(a), LpsValueF32::Vec2(b)) => a == b,
            (LpsValueF32::Vec3(a), LpsValueF32::Vec3(b)) => a == b,
            (LpsValueF32::Vec4(a), LpsValueF32::Vec4(b)) => a == b,
            (LpsValueF32::IVec2(a), LpsValueF32::IVec2(b)) => a == b,
            (LpsValueF32::IVec3(a), LpsValueF32::IVec3(b)) => a == b,
            (LpsValueF32::IVec4(a), LpsValueF32::IVec4(b)) => a == b,
            (LpsValueF32::UVec2(a), LpsValueF32::UVec2(b)) => a == b,
            (LpsValueF32::UVec3(a), LpsValueF32::UVec3(b)) => a == b,
            (LpsValueF32::UVec4(a), LpsValueF32::UVec4(b)) => a == b,
            (LpsValueF32::BVec2(a), LpsValueF32::BVec2(b)) => a == b,
            (LpsValueF32::BVec3(a), LpsValueF32::BVec3(b)) => a == b,
            (LpsValueF32::BVec4(a), LpsValueF32::BVec4(b)) => a == b,
            (LpsValueF32::Mat2x2(a), LpsValueF32::Mat2x2(b)) => a == b,
            (LpsValueF32::Mat3x3(a), LpsValueF32::Mat3x3(b)) => a == b,
            (LpsValueF32::Mat4x4(a), LpsValueF32::Mat4x4(b)) => a == b,
            (LpsValueF32::Array(a), LpsValueF32::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.eq(y))
            }
            (
                LpsValueF32::Struct {
                    name: na,
                    fields: fa,
                },
                LpsValueF32::Struct {
                    name: nb,
                    fields: fb,
                },
            ) => {
                let name_ok = na.is_none() || nb.is_none() || na == nb;
                if !name_ok || fa.len() != fb.len() {
                    return false;
                }
                if fa.iter().all(|(k, _)| k.is_empty()) {
                    return fa.iter().zip(fb.iter()).all(|((_, va), (_, vb))| va.eq(vb));
                }
                fa.iter()
                    .zip(fb.iter())
                    .all(|((ka, va), (kb, vb))| ka == kb && va.eq(vb))
            }
            (LpsValueF32::Texture2D(a), LpsValueF32::Texture2D(b)) => a == b,
            _ => false, // Type mismatch
        }
    }

    /// Approximate equality comparison (~=) with tolerance
    /// For floats: checks if values are within tolerance
    /// For integers and booleans: falls back to exact equality
    /// For vectors/matrices: checks each component within tolerance
    pub fn approx_eq(&self, other: &Self, tolerance: f32) -> bool {
        match (self, other) {
            (LpsValueF32::I32(a), LpsValueF32::I32(b)) => a == b, // Exact for ints
            (LpsValueF32::U32(a), LpsValueF32::U32(b)) => a == b, // Exact for uints
            (LpsValueF32::F32(a), LpsValueF32::F32(b)) => (a - b).abs() <= tolerance,
            (LpsValueF32::Bool(a), LpsValueF32::Bool(b)) => a == b, // Exact for bools
            (LpsValueF32::Vec2(a), LpsValueF32::Vec2(b)) => a
                .iter()
                .zip(b.iter())
                .all(|(x, y)| (x - y).abs() <= tolerance),
            (LpsValueF32::Vec3(a), LpsValueF32::Vec3(b)) => a
                .iter()
                .zip(b.iter())
                .all(|(x, y)| (x - y).abs() <= tolerance),
            (LpsValueF32::Vec4(a), LpsValueF32::Vec4(b)) => a
                .iter()
                .zip(b.iter())
                .all(|(x, y)| (x - y).abs() <= tolerance),
            (LpsValueF32::IVec2(a), LpsValueF32::IVec2(b)) => a == b, // Exact for ints
            (LpsValueF32::IVec3(a), LpsValueF32::IVec3(b)) => a == b, // Exact for ints
            (LpsValueF32::IVec4(a), LpsValueF32::IVec4(b)) => a == b, // Exact for ints
            (LpsValueF32::UVec2(a), LpsValueF32::UVec2(b)) => a == b, // Exact for uints
            (LpsValueF32::UVec3(a), LpsValueF32::UVec3(b)) => a == b, // Exact for uints
            (LpsValueF32::UVec4(a), LpsValueF32::UVec4(b)) => a == b, // Exact for uints
            (LpsValueF32::BVec2(a), LpsValueF32::BVec2(b)) => a == b, // Exact for bools
            (LpsValueF32::BVec3(a), LpsValueF32::BVec3(b)) => a == b, // Exact for bools
            (LpsValueF32::BVec4(a), LpsValueF32::BVec4(b)) => a == b, // Exact for bools
            (LpsValueF32::Mat2x2(a), LpsValueF32::Mat2x2(b)) => a
                .iter()
                .flatten()
                .zip(b.iter().flatten())
                .all(|(x, y)| (x - y).abs() <= tolerance),
            (LpsValueF32::Mat3x3(a), LpsValueF32::Mat3x3(b)) => a
                .iter()
                .flatten()
                .zip(b.iter().flatten())
                .all(|(x, y)| (x - y).abs() <= tolerance),
            (LpsValueF32::Mat4x4(a), LpsValueF32::Mat4x4(b)) => a
                .iter()
                .flatten()
                .zip(b.iter().flatten())
                .all(|(x, y)| (x - y).abs() <= tolerance),
            (LpsValueF32::Array(a), LpsValueF32::Array(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|(x, y)| x.approx_eq(y, tolerance))
            }
            (
                LpsValueF32::Struct {
                    name: na,
                    fields: fa,
                },
                LpsValueF32::Struct {
                    name: nb,
                    fields: fb,
                },
            ) => {
                let name_ok = na.is_none() || nb.is_none() || na == nb;
                if !name_ok || fa.len() != fb.len() {
                    return false;
                }
                if fa.iter().all(|(k, _)| k.is_empty()) {
                    return fa
                        .iter()
                        .zip(fb.iter())
                        .all(|((_, va), (_, vb))| va.approx_eq(vb, tolerance));
                }
                fa.iter()
                    .zip(fb.iter())
                    .all(|((ka, va), (kb, vb))| ka == kb && va.approx_eq(vb, tolerance))
            }
            (LpsValueF32::Texture2D(a), LpsValueF32::Texture2D(b)) => a == b,
            _ => false, // Type mismatch
        }
    }

    /// Default tolerance for float comparisons (1e-4)
    pub const DEFAULT_TOLERANCE: f32 = 1e-4;

    /// Approximate equality with default tolerance
    pub fn approx_eq_default(&self, other: &Self) -> bool {
        self.approx_eq(other, Self::DEFAULT_TOLERANCE)
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::LpsValueF32;

    #[test]
    fn test_flat_array_to_mat2x2_conversion() {
        // Test the conversion logic from test_utils.rs line 71
        // Flat array from emulator (column-major): [col0_row0, col0_row1, col1_row0, col1_row1]
        // For mat2(vec2(1.0, 2.0), vec2(3.0, 4.0)):
        // Storage: [1.0, 2.0, 3.0, 4.0]
        // Conversion: [[v[0], v[1]], [v[2], v[3]]] = [[1.0, 2.0], [3.0, 4.0]]

        let flat_array = vec![1.0, 2.0, 3.0, 4.0];

        // Simulate the conversion from test_utils.rs
        let mat = LpsValueF32::Mat2x2([
            [flat_array[0], flat_array[1]], // [1.0, 2.0] - col 0
            [flat_array[2], flat_array[3]], // [3.0, 4.0] - col 1
        ]);

        // Verify the matrix represents the correct values
        // Column 0 should be [1.0, 2.0], Column 1 should be [3.0, 4.0]
        match mat {
            LpsValueF32::Mat2x2(m) => {
                // m[col][row] format
                // Column 0: [m[0][0], m[0][1]] = [1.0, 2.0] ✓
                assert_eq!(m[0][0], 1.0); // col0_row0
                assert_eq!(m[0][1], 2.0); // col0_row1
                // Column 1: [m[1][0], m[1][1]] = [3.0, 4.0] ✓
                assert_eq!(m[1][0], 3.0); // col1_row0
                assert_eq!(m[1][1], 4.0); // col1_row1
            }
            _ => panic!("Expected Mat2x2"),
        }
    }

    #[test]
    fn test_flat_array_to_mat3x3_conversion() {
        // Test the conversion logic from test_utils.rs line 78
        // Flat array (column-major): [col0_row0, col0_row1, col0_row2, col1_row0, col1_row1, col1_row2, col2_row0, col2_row1, col2_row2]
        // For mat3(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0), vec3(7.0, 8.0, 9.0)):
        // Storage: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]
        // Conversion: [[v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]]

        let flat_array = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];

        // Simulate the conversion from test_utils.rs
        let mat = LpsValueF32::Mat3x3([
            [flat_array[0], flat_array[1], flat_array[2]], // col 0
            [flat_array[3], flat_array[4], flat_array[5]], // col 1
            [flat_array[6], flat_array[7], flat_array[8]], // col 2
        ]);

        // Verify columns are correct
        match mat {
            LpsValueF32::Mat3x3(m) => {
                // Column 0: [1.0, 2.0, 3.0]
                assert_eq!(m[0][0], 1.0);
                assert_eq!(m[0][1], 2.0);
                assert_eq!(m[0][2], 3.0);
                // Column 1: [4.0, 5.0, 6.0]
                assert_eq!(m[1][0], 4.0);
                assert_eq!(m[1][1], 5.0);
                assert_eq!(m[1][2], 6.0);
                // Column 2: [7.0, 8.0, 9.0]
                assert_eq!(m[2][0], 7.0);
                assert_eq!(m[2][1], 8.0);
                assert_eq!(m[2][2], 9.0);
            }
            _ => panic!("Expected Mat3x3"),
        }
    }

    #[test]
    fn test_flat_array_to_mat4x4_conversion() {
        // Test the conversion logic from test_utils.rs lines 85-90
        // Flat array (column-major): 16 elements
        // Conversion pattern: [[v[0], v[1], v[2], v[3]], [v[4], v[5], v[6], v[7]], [v[8], v[9], v[10], v[11]], [v[12], v[13], v[14], v[15]]]

        // Identity matrix
        let flat_array = vec![
            1.0, 0.0, 0.0, 0.0, // column 0
            0.0, 1.0, 0.0, 0.0, // column 1
            0.0, 0.0, 1.0, 0.0, // column 2
            0.0, 0.0, 0.0, 1.0, // column 3
        ];

        // Simulate the conversion from test_utils.rs
        let mat = LpsValueF32::Mat4x4([
            [flat_array[0], flat_array[1], flat_array[2], flat_array[3]], // col 0
            [flat_array[4], flat_array[5], flat_array[6], flat_array[7]], // col 1
            [flat_array[8], flat_array[9], flat_array[10], flat_array[11]], // col 2
            [
                flat_array[12],
                flat_array[13],
                flat_array[14],
                flat_array[15],
            ], // col 3
        ]);

        // Verify columns are correct
        match mat {
            LpsValueF32::Mat4x4(m) => {
                // Column 0: [1.0, 0.0, 0.0, 0.0]
                assert_eq!(m[0][0], 1.0);
                assert_eq!(m[0][1], 0.0);
                assert_eq!(m[0][2], 0.0);
                assert_eq!(m[0][3], 0.0);
                // Column 1: [0.0, 1.0, 0.0, 0.0]
                assert_eq!(m[1][0], 0.0);
                assert_eq!(m[1][1], 1.0);
                assert_eq!(m[1][2], 0.0);
                assert_eq!(m[1][3], 0.0);
                // Column 2: [0.0, 0.0, 1.0, 0.0]
                assert_eq!(m[2][0], 0.0);
                assert_eq!(m[2][1], 0.0);
                assert_eq!(m[2][2], 1.0);
                assert_eq!(m[2][3], 0.0);
                // Column 3: [0.0, 0.0, 0.0, 1.0]
                assert_eq!(m[3][0], 0.0);
                assert_eq!(m[3][1], 0.0);
                assert_eq!(m[3][2], 0.0);
                assert_eq!(m[3][3], 1.0);
            }
            _ => panic!("Expected Mat4x4"),
        }
    }
}
