use core::ops::{Add, Div, Mul, Neg, Sub};

use crate::glsl::q32::types::mat3_q32::Mat3Q32;
use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec4_q32::Vec4Q32;

/// 4x4 matrix for Q32 fixed-point arithmetic (GLSL-compatible, column-major storage)
///
/// Storage layout (column-major):
/// [m00, m10, m20, m30, m01, m11, m21, m31, m02, m12, m22, m32, m03, m13, m23, m33]
/// Where m[row][col] represents the element at row `row` and column `col`
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Mat4Q32 {
    // Column-major storage: [col0, col1, col2, col3] where each column is [x, y, z, w]
    // Storage: [m00, m10, m20, m30, m01, m11, m21, m31, m02, m12, m22, m32, m03, m13, m23, m33]
    pub m: [Q32; 16],
}

impl Mat4Q32 {
    /// Create a new matrix from 16 Q32 values (column-major order)
    ///
    /// Parameters are in column-major order:
    /// m00, m10, m20, m30, m01, m11, m21, m31, m02, m12, m22, m32, m03, m13, m23, m33
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub const fn new(
        m00: Q32,
        m10: Q32,
        m20: Q32,
        m30: Q32,
        m01: Q32,
        m11: Q32,
        m21: Q32,
        m31: Q32,
        m02: Q32,
        m12: Q32,
        m22: Q32,
        m32: Q32,
        m03: Q32,
        m13: Q32,
        m23: Q32,
        m33: Q32,
    ) -> Self {
        Mat4Q32 {
            m: [
                m00, m10, m20, m30, m01, m11, m21, m31, m02, m12, m22, m32, m03, m13, m23, m33,
            ],
        }
    }

    /// Create a matrix from 16 f32 values (column-major order)
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn from_f32(
        m00: f32,
        m10: f32,
        m20: f32,
        m30: f32,
        m01: f32,
        m11: f32,
        m21: f32,
        m31: f32,
        m02: f32,
        m12: f32,
        m22: f32,
        m32: f32,
        m03: f32,
        m13: f32,
        m23: f32,
        m33: f32,
    ) -> Self {
        Mat4Q32::new(
            Q32::from_f32(m00),
            Q32::from_f32(m10),
            Q32::from_f32(m20),
            Q32::from_f32(m30),
            Q32::from_f32(m01),
            Q32::from_f32(m11),
            Q32::from_f32(m21),
            Q32::from_f32(m31),
            Q32::from_f32(m02),
            Q32::from_f32(m12),
            Q32::from_f32(m22),
            Q32::from_f32(m32),
            Q32::from_f32(m03),
            Q32::from_f32(m13),
            Q32::from_f32(m23),
            Q32::from_f32(m33),
        )
    }

    /// Create a matrix from 4 Vec4Q32 columns
    #[inline(always)]
    pub fn from_vec4(col0: Vec4Q32, col1: Vec4Q32, col2: Vec4Q32, col3: Vec4Q32) -> Self {
        Mat4Q32::new(
            col0.x, col0.y, col0.z, col0.w, col1.x, col1.y, col1.z, col1.w, col2.x, col2.y, col2.z,
            col2.w, col3.x, col3.y, col3.z, col3.w,
        )
    }

    /// Create identity matrix
    #[inline(always)]
    pub const fn identity() -> Self {
        Mat4Q32::new(
            Q32::ONE,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ONE,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ONE,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ONE,
        )
    }

    /// Create zero matrix
    #[inline(always)]
    pub const fn zero() -> Self {
        Mat4Q32::new(
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
        )
    }

    /// Get element at row `row` and column `col`
    #[inline(always)]
    pub fn get(self, row: usize, col: usize) -> Q32 {
        self.m[col * 4 + row]
    }

    /// Set element at row `row` and column `col`
    #[inline(always)]
    pub fn set(&mut self, row: usize, col: usize, value: Q32) {
        self.m[col * 4 + row] = value;
    }

    /// Get column 0 as Vec4Q32
    #[inline(always)]
    pub fn col0(self) -> Vec4Q32 {
        Vec4Q32::new(self.m[0], self.m[1], self.m[2], self.m[3])
    }

    /// Get column 1 as Vec4Q32
    #[inline(always)]
    pub fn col1(self) -> Vec4Q32 {
        Vec4Q32::new(self.m[4], self.m[5], self.m[6], self.m[7])
    }

    /// Get column 2 as Vec4Q32
    #[inline(always)]
    pub fn col2(self) -> Vec4Q32 {
        Vec4Q32::new(self.m[8], self.m[9], self.m[10], self.m[11])
    }

    /// Get column 3 as Vec4Q32
    #[inline(always)]
    pub fn col3(self) -> Vec4Q32 {
        Vec4Q32::new(self.m[12], self.m[13], self.m[14], self.m[15])
    }

    /// Matrix-matrix multiplication
    #[allow(clippy::should_implement_trait)]
    #[inline(always)]
    pub fn mul(self, rhs: Self) -> Self {
        let a = self;
        let b = rhs;
        Mat4Q32::new(
            // Row 0
            a.m[0] * b.m[0] + a.m[4] * b.m[1] + a.m[8] * b.m[2] + a.m[12] * b.m[3],
            a.m[1] * b.m[0] + a.m[5] * b.m[1] + a.m[9] * b.m[2] + a.m[13] * b.m[3],
            a.m[2] * b.m[0] + a.m[6] * b.m[1] + a.m[10] * b.m[2] + a.m[14] * b.m[3],
            a.m[3] * b.m[0] + a.m[7] * b.m[1] + a.m[11] * b.m[2] + a.m[15] * b.m[3],
            // Row 1
            a.m[0] * b.m[4] + a.m[4] * b.m[5] + a.m[8] * b.m[6] + a.m[12] * b.m[7],
            a.m[1] * b.m[4] + a.m[5] * b.m[5] + a.m[9] * b.m[6] + a.m[13] * b.m[7],
            a.m[2] * b.m[4] + a.m[6] * b.m[5] + a.m[10] * b.m[6] + a.m[14] * b.m[7],
            a.m[3] * b.m[4] + a.m[7] * b.m[5] + a.m[11] * b.m[6] + a.m[15] * b.m[7],
            // Row 2
            a.m[0] * b.m[8] + a.m[4] * b.m[9] + a.m[8] * b.m[10] + a.m[12] * b.m[11],
            a.m[1] * b.m[8] + a.m[5] * b.m[9] + a.m[9] * b.m[10] + a.m[13] * b.m[11],
            a.m[2] * b.m[8] + a.m[6] * b.m[9] + a.m[10] * b.m[10] + a.m[14] * b.m[11],
            a.m[3] * b.m[8] + a.m[7] * b.m[9] + a.m[11] * b.m[10] + a.m[15] * b.m[11],
            // Row 3
            a.m[0] * b.m[12] + a.m[4] * b.m[13] + a.m[8] * b.m[14] + a.m[12] * b.m[15],
            a.m[1] * b.m[12] + a.m[5] * b.m[13] + a.m[9] * b.m[14] + a.m[13] * b.m[15],
            a.m[2] * b.m[12] + a.m[6] * b.m[13] + a.m[10] * b.m[14] + a.m[14] * b.m[15],
            a.m[3] * b.m[12] + a.m[7] * b.m[13] + a.m[11] * b.m[14] + a.m[15] * b.m[15],
        )
    }

    /// Matrix-vector multiplication (mat4 * vec4)
    #[inline(always)]
    pub fn mul_vec4(self, v: Vec4Q32) -> Vec4Q32 {
        Vec4Q32::new(
            self.m[0] * v.x + self.m[4] * v.y + self.m[8] * v.z + self.m[12] * v.w,
            self.m[1] * v.x + self.m[5] * v.y + self.m[9] * v.z + self.m[13] * v.w,
            self.m[2] * v.x + self.m[6] * v.y + self.m[10] * v.z + self.m[14] * v.w,
            self.m[3] * v.x + self.m[7] * v.y + self.m[11] * v.z + self.m[15] * v.w,
        )
    }

    /// Transpose matrix
    #[inline(always)]
    pub fn transpose(self) -> Self {
        Mat4Q32::new(
            self.m[0], self.m[4], self.m[8], self.m[12], self.m[1], self.m[5], self.m[9],
            self.m[13], self.m[2], self.m[6], self.m[10], self.m[14], self.m[3], self.m[7],
            self.m[11], self.m[15],
        )
    }

    /// Calculate determinant using Laplace expansion
    #[inline(always)]
    pub fn determinant(self) -> Q32 {
        let m = &self.m;
        // Laplace expansion along first row
        let a = m[0]
            * Mat3Q32::new(m[5], m[6], m[7], m[9], m[10], m[11], m[13], m[14], m[15]).determinant();
        let b = m[4]
            * Mat3Q32::new(m[1], m[2], m[3], m[9], m[10], m[11], m[13], m[14], m[15]).determinant();
        let c = m[8]
            * Mat3Q32::new(m[1], m[2], m[3], m[5], m[6], m[7], m[13], m[14], m[15]).determinant();
        let d = m[12]
            * Mat3Q32::new(m[1], m[2], m[3], m[5], m[6], m[7], m[9], m[10], m[11]).determinant();
        a - b + c - d
    }

    /// Calculate inverse matrix
    ///
    /// Returns None if matrix is singular (determinant is zero)
    #[inline(always)]
    pub fn inverse(self) -> Option<Self> {
        let det = self.determinant();
        if det.to_fixed() == 0 {
            return None;
        }

        let m = &self.m;
        // Calculate cofactor matrix (transposed for adjugate)
        // Using 3x3 determinants for each cofactor
        let c00 =
            Mat3Q32::new(m[5], m[6], m[7], m[9], m[10], m[11], m[13], m[14], m[15]).determinant();
        let c01 =
            -Mat3Q32::new(m[1], m[2], m[3], m[9], m[10], m[11], m[13], m[14], m[15]).determinant();
        let c02 =
            Mat3Q32::new(m[1], m[2], m[3], m[5], m[6], m[7], m[13], m[14], m[15]).determinant();
        let c03 =
            -Mat3Q32::new(m[1], m[2], m[3], m[5], m[6], m[7], m[9], m[10], m[11]).determinant();

        let c10 =
            -Mat3Q32::new(m[4], m[6], m[7], m[8], m[10], m[11], m[12], m[14], m[15]).determinant();
        let c11 =
            Mat3Q32::new(m[0], m[2], m[3], m[8], m[10], m[11], m[12], m[14], m[15]).determinant();
        let c12 =
            -Mat3Q32::new(m[0], m[2], m[3], m[4], m[6], m[7], m[12], m[14], m[15]).determinant();
        let c13 =
            Mat3Q32::new(m[0], m[2], m[3], m[4], m[6], m[7], m[8], m[10], m[11]).determinant();

        let c20 =
            Mat3Q32::new(m[4], m[5], m[7], m[8], m[9], m[11], m[12], m[13], m[15]).determinant();
        let c21 =
            -Mat3Q32::new(m[0], m[1], m[3], m[8], m[9], m[11], m[12], m[13], m[15]).determinant();
        let c22 =
            Mat3Q32::new(m[0], m[1], m[3], m[4], m[5], m[7], m[12], m[13], m[15]).determinant();
        let c23 =
            -Mat3Q32::new(m[0], m[1], m[3], m[4], m[5], m[7], m[8], m[9], m[11]).determinant();

        let c30 =
            -Mat3Q32::new(m[4], m[5], m[6], m[8], m[9], m[10], m[12], m[13], m[14]).determinant();
        let c31 =
            Mat3Q32::new(m[0], m[1], m[2], m[8], m[9], m[10], m[12], m[13], m[14]).determinant();
        let c32 =
            -Mat3Q32::new(m[0], m[1], m[2], m[4], m[5], m[6], m[12], m[13], m[14]).determinant();
        let c33 = Mat3Q32::new(m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10]).determinant();

        // Adjugate matrix (transpose of cofactor) divided by determinant
        let inv_det = Q32::ONE / det;
        Some(Mat4Q32::new(
            c00 * inv_det,
            c10 * inv_det,
            c20 * inv_det,
            c30 * inv_det,
            c01 * inv_det,
            c11 * inv_det,
            c21 * inv_det,
            c31 * inv_det,
            c02 * inv_det,
            c12 * inv_det,
            c22 * inv_det,
            c32 * inv_det,
            c03 * inv_det,
            c13 * inv_det,
            c23 * inv_det,
            c33 * inv_det,
        ))
    }
}

// Matrix + Matrix
impl Add for Mat4Q32 {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: Self) -> Self {
        Mat4Q32::new(
            self.m[0] + rhs.m[0],
            self.m[1] + rhs.m[1],
            self.m[2] + rhs.m[2],
            self.m[3] + rhs.m[3],
            self.m[4] + rhs.m[4],
            self.m[5] + rhs.m[5],
            self.m[6] + rhs.m[6],
            self.m[7] + rhs.m[7],
            self.m[8] + rhs.m[8],
            self.m[9] + rhs.m[9],
            self.m[10] + rhs.m[10],
            self.m[11] + rhs.m[11],
            self.m[12] + rhs.m[12],
            self.m[13] + rhs.m[13],
            self.m[14] + rhs.m[14],
            self.m[15] + rhs.m[15],
        )
    }
}

// Matrix - Matrix
impl Sub for Mat4Q32 {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: Self) -> Self {
        Mat4Q32::new(
            self.m[0] - rhs.m[0],
            self.m[1] - rhs.m[1],
            self.m[2] - rhs.m[2],
            self.m[3] - rhs.m[3],
            self.m[4] - rhs.m[4],
            self.m[5] - rhs.m[5],
            self.m[6] - rhs.m[6],
            self.m[7] - rhs.m[7],
            self.m[8] - rhs.m[8],
            self.m[9] - rhs.m[9],
            self.m[10] - rhs.m[10],
            self.m[11] - rhs.m[11],
            self.m[12] - rhs.m[12],
            self.m[13] - rhs.m[13],
            self.m[14] - rhs.m[14],
            self.m[15] - rhs.m[15],
        )
    }
}

// Matrix * Matrix (matrix multiplication)
impl Mul for Mat4Q32 {
    type Output = Self;

    #[inline(always)]
    fn mul(self, rhs: Self) -> Self {
        self.mul(rhs)
    }
}

// Matrix * Vec4 (matrix-vector multiplication)
impl Mul<Vec4Q32> for Mat4Q32 {
    type Output = Vec4Q32;

    #[inline(always)]
    fn mul(self, rhs: Vec4Q32) -> Vec4Q32 {
        self.mul_vec4(rhs)
    }
}

// Matrix * Scalar
impl Mul<Q32> for Mat4Q32 {
    type Output = Self;

    #[inline(always)]
    fn mul(self, rhs: Q32) -> Self {
        Mat4Q32::new(
            self.m[0] * rhs,
            self.m[1] * rhs,
            self.m[2] * rhs,
            self.m[3] * rhs,
            self.m[4] * rhs,
            self.m[5] * rhs,
            self.m[6] * rhs,
            self.m[7] * rhs,
            self.m[8] * rhs,
            self.m[9] * rhs,
            self.m[10] * rhs,
            self.m[11] * rhs,
            self.m[12] * rhs,
            self.m[13] * rhs,
            self.m[14] * rhs,
            self.m[15] * rhs,
        )
    }
}

// Matrix / Scalar
impl Div<Q32> for Mat4Q32 {
    type Output = Self;

    #[inline(always)]
    fn div(self, rhs: Q32) -> Self {
        Mat4Q32::new(
            self.m[0] / rhs,
            self.m[1] / rhs,
            self.m[2] / rhs,
            self.m[3] / rhs,
            self.m[4] / rhs,
            self.m[5] / rhs,
            self.m[6] / rhs,
            self.m[7] / rhs,
            self.m[8] / rhs,
            self.m[9] / rhs,
            self.m[10] / rhs,
            self.m[11] / rhs,
            self.m[12] / rhs,
            self.m[13] / rhs,
            self.m[14] / rhs,
            self.m[15] / rhs,
        )
    }
}

// -Matrix
impl Neg for Mat4Q32 {
    type Output = Self;

    #[inline(always)]
    fn neg(self) -> Self {
        Mat4Q32::new(
            -self.m[0],
            -self.m[1],
            -self.m[2],
            -self.m[3],
            -self.m[4],
            -self.m[5],
            -self.m[6],
            -self.m[7],
            -self.m[8],
            -self.m[9],
            -self.m[10],
            -self.m[11],
            -self.m[12],
            -self.m[13],
            -self.m[14],
            -self.m[15],
        )
    }
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    #[test]
    fn test_identity() {
        let m = Mat4Q32::identity();
        assert_eq!(m.get(0, 0).to_f32(), 1.0);
        assert_eq!(m.get(1, 1).to_f32(), 1.0);
        assert_eq!(m.get(2, 2).to_f32(), 1.0);
        assert_eq!(m.get(3, 3).to_f32(), 1.0);
        assert_eq!(m.get(0, 1).to_f32(), 0.0);
        assert_eq!(m.get(1, 0).to_f32(), 0.0);
    }

    #[test]
    fn test_zero() {
        let m = Mat4Q32::zero();
        for i in 0..16 {
            assert_eq!(m.m[i].to_f32(), 0.0);
        }
    }

    #[test]
    fn test_new() {
        let m = Mat4Q32::new(
            Q32::from_i32(1),
            Q32::from_i32(2),
            Q32::from_i32(3),
            Q32::from_i32(4),
            Q32::from_i32(5),
            Q32::from_i32(6),
            Q32::from_i32(7),
            Q32::from_i32(8),
            Q32::from_i32(9),
            Q32::from_i32(10),
            Q32::from_i32(11),
            Q32::from_i32(12),
            Q32::from_i32(13),
            Q32::from_i32(14),
            Q32::from_i32(15),
            Q32::from_i32(16),
        );
        assert_eq!(m.get(0, 0).to_f32(), 1.0);
        assert_eq!(m.get(3, 3).to_f32(), 16.0);
    }

    #[test]
    fn test_from_f32() {
        let m = Mat4Q32::from_f32(
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        );
        assert_eq!(m.get(0, 0).to_f32(), 1.0);
        assert_eq!(m.get(3, 3).to_f32(), 16.0);
    }

    #[test]
    fn test_from_vec4() {
        let col0 = Vec4Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        let col1 = Vec4Q32::from_f32(5.0, 6.0, 7.0, 8.0);
        let col2 = Vec4Q32::from_f32(9.0, 10.0, 11.0, 12.0);
        let col3 = Vec4Q32::from_f32(13.0, 14.0, 15.0, 16.0);
        let m = Mat4Q32::from_vec4(col0, col1, col2, col3);
        assert_eq!(m.col0(), col0);
        assert_eq!(m.col1(), col1);
        assert_eq!(m.col2(), col2);
        assert_eq!(m.col3(), col3);
    }

    #[test]
    fn test_get_set() {
        let mut m = Mat4Q32::zero();
        m.set(0, 0, Q32::from_f32(5.0));
        assert_eq!(m.get(0, 0).to_f32(), 5.0);
    }

    #[test]
    fn test_col0_col1_col2_col3() {
        let m = Mat4Q32::from_f32(
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        );
        let col0 = m.col0();
        assert_eq!(col0.x.to_f32(), 1.0);
        assert_eq!(col0.y.to_f32(), 2.0);
        assert_eq!(col0.z.to_f32(), 3.0);
        assert_eq!(col0.w.to_f32(), 4.0);
    }

    #[test]
    fn test_add() {
        let a = Mat4Q32::from_f32(
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        );
        let b = Mat4Q32::from_f32(
            1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        );
        let c = a + b;
        assert_eq!(c.get(0, 0).to_f32(), 2.0);
        assert_eq!(c.get(3, 3).to_f32(), 17.0);
    }

    #[test]
    fn test_sub() {
        let a = Mat4Q32::from_f32(
            5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0,
        );
        let b = Mat4Q32::from_f32(
            1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        );
        let c = a - b;
        assert_eq!(c.get(0, 0).to_f32(), 4.0);
    }

    #[test]
    fn test_mul_matrix() {
        let a = Mat4Q32::identity();
        let b = Mat4Q32::identity();
        let c = a * b;
        assert_eq!(c, Mat4Q32::identity());
    }

    #[test]
    fn test_mul_vec4() {
        let m = Mat4Q32::identity();
        let v = Vec4Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        let result = m * v;
        assert_eq!(result.x.to_f32(), 1.0);
        assert_eq!(result.y.to_f32(), 2.0);
        assert_eq!(result.z.to_f32(), 3.0);
        assert_eq!(result.w.to_f32(), 4.0);
    }

    #[test]
    fn test_mul_scalar() {
        let m = Mat4Q32::from_f32(
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        );
        let s = Q32::from_f32(2.0);
        let result = m * s;
        assert_eq!(result.get(0, 0).to_f32(), 2.0);
        assert_eq!(result.get(3, 3).to_f32(), 32.0);
    }

    #[test]
    fn test_div_scalar() {
        let m = Mat4Q32::from_f32(
            4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 30.0, 32.0,
            34.0,
        );
        let s = Q32::from_f32(2.0);
        let result = m / s;
        assert_eq!(result.get(0, 0).to_f32(), 2.0);
        assert_eq!(result.get(3, 3).to_f32(), 17.0);
    }

    #[test]
    fn test_neg() {
        let m = Mat4Q32::from_f32(
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        );
        let neg = -m;
        assert_eq!(neg.get(0, 0).to_f32(), -1.0);
        assert_eq!(neg.get(3, 3).to_f32(), -16.0);
    }

    #[test]
    fn test_transpose() {
        let m = Mat4Q32::from_f32(
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        );
        let t = m.transpose();
        assert_eq!(t.get(0, 1).to_f32(), m.get(1, 0).to_f32());
        assert_eq!(t.get(1, 0).to_f32(), m.get(0, 1).to_f32());
    }

    #[test]
    fn test_determinant() {
        let m = Mat4Q32::identity();
        let det = m.determinant();
        assert!((det.to_f32() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_inverse() {
        let m = Mat4Q32::identity();
        let inv = m.inverse().unwrap();
        assert_eq!(inv, Mat4Q32::identity());
    }

    #[test]
    fn test_inverse_singular() {
        let m = Mat4Q32::zero();
        assert_eq!(m.inverse(), None);
    }

    #[test]
    fn test_inverse_product() {
        // Test that m * m.inverse() = identity (approximately)
        // Use a simple diagonal matrix for easier verification
        let m = Mat4Q32::from_f32(
            2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0,
        );
        let inv = m.inverse().unwrap();
        let product = m * inv;
        // Should be approximately identity
        assert!((product.get(0, 0).to_f32() - 1.0).abs() < 0.01);
        assert!((product.get(1, 1).to_f32() - 1.0).abs() < 0.01);
        assert!((product.get(2, 2).to_f32() - 1.0).abs() < 0.01);
        assert!((product.get(3, 3).to_f32() - 1.0).abs() < 0.01);
        assert!((product.get(0, 1).to_f32() - 0.0).abs() < 0.01);
    }
}
