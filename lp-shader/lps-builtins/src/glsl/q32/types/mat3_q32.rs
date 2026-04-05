use core::ops::{Add, Div, Mul, Neg, Sub};

use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;

/// 3x3 matrix for Q32 fixed-point arithmetic (GLSL-compatible, column-major storage)
///
/// Storage layout (column-major):
/// [m00, m10, m20, m01, m11, m21, m02, m12, m22]
/// Where m[row][col] represents the element at row `row` and column `col`
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Mat3Q32 {
    // Column-major storage: [col0, col1, col2] where each column is [x, y, z]
    // Storage: [m00, m10, m20, m01, m11, m21, m02, m12, m22]
    pub m: [Q32; 9],
}

impl Mat3Q32 {
    /// Create a new matrix from 9 Q32 values (column-major order)
    ///
    /// Parameters are in column-major order:
    /// m00, m10, m20, m01, m11, m21, m02, m12, m22
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub const fn new(
        m00: Q32,
        m10: Q32,
        m20: Q32,
        m01: Q32,
        m11: Q32,
        m21: Q32,
        m02: Q32,
        m12: Q32,
        m22: Q32,
    ) -> Self {
        Mat3Q32 {
            m: [m00, m10, m20, m01, m11, m21, m02, m12, m22],
        }
    }

    /// Create a matrix from 9 f32 values (column-major order)
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn from_f32(
        m00: f32,
        m10: f32,
        m20: f32,
        m01: f32,
        m11: f32,
        m21: f32,
        m02: f32,
        m12: f32,
        m22: f32,
    ) -> Self {
        Mat3Q32::new(
            Q32::from_f32(m00),
            Q32::from_f32(m10),
            Q32::from_f32(m20),
            Q32::from_f32(m01),
            Q32::from_f32(m11),
            Q32::from_f32(m21),
            Q32::from_f32(m02),
            Q32::from_f32(m12),
            Q32::from_f32(m22),
        )
    }

    /// Create a matrix from 3 Vec3Q32 columns
    #[inline(always)]
    pub fn from_vec3(col0: Vec3Q32, col1: Vec3Q32, col2: Vec3Q32) -> Self {
        Mat3Q32::new(
            col0.x, col0.y, col0.z, col1.x, col1.y, col1.z, col2.x, col2.y, col2.z,
        )
    }

    /// Create identity matrix
    #[inline(always)]
    pub const fn identity() -> Self {
        Mat3Q32::new(
            Q32::ONE,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ONE,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ZERO,
            Q32::ONE,
        )
    }

    /// Create zero matrix
    #[inline(always)]
    pub const fn zero() -> Self {
        Mat3Q32::new(
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
        self.m[col * 3 + row]
    }

    /// Set element at row `row` and column `col`
    #[inline(always)]
    pub fn set(&mut self, row: usize, col: usize, value: Q32) {
        self.m[col * 3 + row] = value;
    }

    /// Get column 0 as Vec3Q32
    #[inline(always)]
    pub fn col0(self) -> Vec3Q32 {
        Vec3Q32::new(self.m[0], self.m[1], self.m[2])
    }

    /// Get column 1 as Vec3Q32
    #[inline(always)]
    pub fn col1(self) -> Vec3Q32 {
        Vec3Q32::new(self.m[3], self.m[4], self.m[5])
    }

    /// Get column 2 as Vec3Q32
    #[inline(always)]
    pub fn col2(self) -> Vec3Q32 {
        Vec3Q32::new(self.m[6], self.m[7], self.m[8])
    }

    /// Matrix-matrix multiplication
    #[allow(clippy::should_implement_trait)]
    #[inline(always)]
    pub fn mul(self, rhs: Self) -> Self {
        let a = self;
        let b = rhs;
        Mat3Q32::new(
            // Row 0
            a.m[0] * b.m[0] + a.m[3] * b.m[1] + a.m[6] * b.m[2],
            a.m[1] * b.m[0] + a.m[4] * b.m[1] + a.m[7] * b.m[2],
            a.m[2] * b.m[0] + a.m[5] * b.m[1] + a.m[8] * b.m[2],
            // Row 1
            a.m[0] * b.m[3] + a.m[3] * b.m[4] + a.m[6] * b.m[5],
            a.m[1] * b.m[3] + a.m[4] * b.m[4] + a.m[7] * b.m[5],
            a.m[2] * b.m[3] + a.m[5] * b.m[4] + a.m[8] * b.m[5],
            // Row 2
            a.m[0] * b.m[6] + a.m[3] * b.m[7] + a.m[6] * b.m[8],
            a.m[1] * b.m[6] + a.m[4] * b.m[7] + a.m[7] * b.m[8],
            a.m[2] * b.m[6] + a.m[5] * b.m[7] + a.m[8] * b.m[8],
        )
    }

    /// Matrix-vector multiplication (mat3 * vec3)
    #[inline(always)]
    pub fn mul_vec3(self, v: Vec3Q32) -> Vec3Q32 {
        Vec3Q32::new(
            self.m[0] * v.x + self.m[3] * v.y + self.m[6] * v.z,
            self.m[1] * v.x + self.m[4] * v.y + self.m[7] * v.z,
            self.m[2] * v.x + self.m[5] * v.y + self.m[8] * v.z,
        )
    }

    /// Transpose matrix
    #[inline(always)]
    pub fn transpose(self) -> Self {
        Mat3Q32::new(
            self.m[0], self.m[3], self.m[6], self.m[1], self.m[4], self.m[7], self.m[2], self.m[5],
            self.m[8],
        )
    }

    /// Calculate determinant
    #[inline(always)]
    pub fn determinant(self) -> Q32 {
        let m = &self.m;
        // Using Sarrus' rule for 3x3 determinant
        let a = m[0] * m[4] * m[8];
        let b = m[1] * m[5] * m[6];
        let c = m[2] * m[3] * m[7];
        let d = m[2] * m[4] * m[6];
        let e = m[0] * m[5] * m[7];
        let f = m[1] * m[3] * m[8];
        (a + b + c) - (d + e + f)
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
        let c00 = m[4] * m[8] - m[5] * m[7];
        let c01 = -(m[1] * m[8] - m[2] * m[7]);
        let c02 = m[1] * m[5] - m[2] * m[4];
        let c10 = -(m[3] * m[8] - m[5] * m[6]);
        let c11 = m[0] * m[8] - m[2] * m[6];
        let c12 = -(m[0] * m[5] - m[2] * m[3]);
        let c20 = m[3] * m[7] - m[4] * m[6];
        let c21 = -(m[0] * m[7] - m[1] * m[6]);
        let c22 = m[0] * m[4] - m[1] * m[3];

        // Adjugate matrix (transpose of cofactor) divided by determinant
        let inv_det = Q32::ONE / det;
        Some(Mat3Q32::new(
            c00 * inv_det,
            c01 * inv_det,
            c02 * inv_det,
            c10 * inv_det,
            c11 * inv_det,
            c12 * inv_det,
            c20 * inv_det,
            c21 * inv_det,
            c22 * inv_det,
        ))
    }
}

// Matrix + Matrix
impl Add for Mat3Q32 {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: Self) -> Self {
        Mat3Q32::new(
            self.m[0] + rhs.m[0],
            self.m[1] + rhs.m[1],
            self.m[2] + rhs.m[2],
            self.m[3] + rhs.m[3],
            self.m[4] + rhs.m[4],
            self.m[5] + rhs.m[5],
            self.m[6] + rhs.m[6],
            self.m[7] + rhs.m[7],
            self.m[8] + rhs.m[8],
        )
    }
}

// Matrix - Matrix
impl Sub for Mat3Q32 {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: Self) -> Self {
        Mat3Q32::new(
            self.m[0] - rhs.m[0],
            self.m[1] - rhs.m[1],
            self.m[2] - rhs.m[2],
            self.m[3] - rhs.m[3],
            self.m[4] - rhs.m[4],
            self.m[5] - rhs.m[5],
            self.m[6] - rhs.m[6],
            self.m[7] - rhs.m[7],
            self.m[8] - rhs.m[8],
        )
    }
}

// Matrix * Matrix (matrix multiplication)
impl Mul for Mat3Q32 {
    type Output = Self;

    #[inline(always)]
    fn mul(self, rhs: Self) -> Self {
        self.mul(rhs)
    }
}

// Matrix * Vec3 (matrix-vector multiplication)
impl Mul<Vec3Q32> for Mat3Q32 {
    type Output = Vec3Q32;

    #[inline(always)]
    fn mul(self, rhs: Vec3Q32) -> Vec3Q32 {
        self.mul_vec3(rhs)
    }
}

// Matrix * Scalar
impl Mul<Q32> for Mat3Q32 {
    type Output = Self;

    #[inline(always)]
    fn mul(self, rhs: Q32) -> Self {
        Mat3Q32::new(
            self.m[0] * rhs,
            self.m[1] * rhs,
            self.m[2] * rhs,
            self.m[3] * rhs,
            self.m[4] * rhs,
            self.m[5] * rhs,
            self.m[6] * rhs,
            self.m[7] * rhs,
            self.m[8] * rhs,
        )
    }
}

// Matrix / Scalar
impl Div<Q32> for Mat3Q32 {
    type Output = Self;

    #[inline(always)]
    fn div(self, rhs: Q32) -> Self {
        Mat3Q32::new(
            self.m[0] / rhs,
            self.m[1] / rhs,
            self.m[2] / rhs,
            self.m[3] / rhs,
            self.m[4] / rhs,
            self.m[5] / rhs,
            self.m[6] / rhs,
            self.m[7] / rhs,
            self.m[8] / rhs,
        )
    }
}

// -Matrix
impl Neg for Mat3Q32 {
    type Output = Self;

    #[inline(always)]
    fn neg(self) -> Self {
        Mat3Q32::new(
            -self.m[0], -self.m[1], -self.m[2], -self.m[3], -self.m[4], -self.m[5], -self.m[6],
            -self.m[7], -self.m[8],
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
        let m = Mat3Q32::identity();
        assert_eq!(m.get(0, 0).to_f32(), 1.0);
        assert_eq!(m.get(1, 1).to_f32(), 1.0);
        assert_eq!(m.get(2, 2).to_f32(), 1.0);
        assert_eq!(m.get(0, 1).to_f32(), 0.0);
        assert_eq!(m.get(1, 0).to_f32(), 0.0);
    }

    #[test]
    fn test_zero() {
        let m = Mat3Q32::zero();
        for i in 0..9 {
            assert_eq!(m.m[i].to_f32(), 0.0);
        }
    }

    #[test]
    fn test_new() {
        let m = Mat3Q32::new(
            Q32::from_i32(1),
            Q32::from_i32(2),
            Q32::from_i32(3),
            Q32::from_i32(4),
            Q32::from_i32(5),
            Q32::from_i32(6),
            Q32::from_i32(7),
            Q32::from_i32(8),
            Q32::from_i32(9),
        );
        assert_eq!(m.get(0, 0).to_f32(), 1.0);
        assert_eq!(m.get(2, 2).to_f32(), 9.0);
    }

    #[test]
    fn test_from_f32() {
        let m = Mat3Q32::from_f32(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
        assert_eq!(m.get(0, 0).to_f32(), 1.0);
        assert_eq!(m.get(2, 2).to_f32(), 9.0);
    }

    #[test]
    fn test_from_vec3() {
        let col0 = Vec3Q32::from_f32(1.0, 2.0, 3.0);
        let col1 = Vec3Q32::from_f32(4.0, 5.0, 6.0);
        let col2 = Vec3Q32::from_f32(7.0, 8.0, 9.0);
        let m = Mat3Q32::from_vec3(col0, col1, col2);
        assert_eq!(m.col0(), col0);
        assert_eq!(m.col1(), col1);
        assert_eq!(m.col2(), col2);
    }

    #[test]
    fn test_get_set() {
        let mut m = Mat3Q32::zero();
        m.set(0, 0, Q32::from_f32(5.0));
        assert_eq!(m.get(0, 0).to_f32(), 5.0);
    }

    #[test]
    fn test_col0_col1_col2() {
        let m = Mat3Q32::from_f32(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
        let col0 = m.col0();
        assert_eq!(col0.x.to_f32(), 1.0);
        assert_eq!(col0.y.to_f32(), 2.0);
        assert_eq!(col0.z.to_f32(), 3.0);
    }

    #[test]
    fn test_add() {
        let a = Mat3Q32::from_f32(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
        let b = Mat3Q32::from_f32(1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0);
        let c = a + b;
        assert_eq!(c.get(0, 0).to_f32(), 2.0);
        assert_eq!(c.get(2, 2).to_f32(), 10.0);
    }

    #[test]
    fn test_sub() {
        let a = Mat3Q32::from_f32(5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0);
        let b = Mat3Q32::from_f32(1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0);
        let c = a - b;
        assert_eq!(c.get(0, 0).to_f32(), 4.0);
    }

    #[test]
    fn test_mul_matrix() {
        let a = Mat3Q32::identity();
        let b = Mat3Q32::identity();
        let c = a * b;
        assert_eq!(c, Mat3Q32::identity());
    }

    #[test]
    fn test_mul_vec3() {
        let m = Mat3Q32::identity();
        let v = Vec3Q32::from_f32(1.0, 2.0, 3.0);
        let result = m * v;
        assert_eq!(result.x.to_f32(), 1.0);
        assert_eq!(result.y.to_f32(), 2.0);
        assert_eq!(result.z.to_f32(), 3.0);
    }

    #[test]
    fn test_mul_scalar() {
        let m = Mat3Q32::from_f32(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
        let s = Q32::from_f32(2.0);
        let result = m * s;
        assert_eq!(result.get(0, 0).to_f32(), 2.0);
        assert_eq!(result.get(2, 2).to_f32(), 18.0);
    }

    #[test]
    fn test_div_scalar() {
        let m = Mat3Q32::from_f32(4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0);
        let s = Q32::from_f32(2.0);
        let result = m / s;
        assert_eq!(result.get(0, 0).to_f32(), 2.0);
        assert_eq!(result.get(2, 2).to_f32(), 10.0);
    }

    #[test]
    fn test_neg() {
        let m = Mat3Q32::from_f32(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
        let neg = -m;
        assert_eq!(neg.get(0, 0).to_f32(), -1.0);
        assert_eq!(neg.get(2, 2).to_f32(), -9.0);
    }

    #[test]
    fn test_transpose() {
        let m = Mat3Q32::from_f32(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
        let t = m.transpose();
        assert_eq!(t.get(0, 1).to_f32(), m.get(1, 0).to_f32());
        assert_eq!(t.get(1, 0).to_f32(), m.get(0, 1).to_f32());
    }

    #[test]
    fn test_determinant() {
        let m = Mat3Q32::identity();
        let det = m.determinant();
        assert!((det.to_f32() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_inverse() {
        let m = Mat3Q32::identity();
        let inv = m.inverse().unwrap();
        assert_eq!(inv, Mat3Q32::identity());
    }

    #[test]
    fn test_inverse_singular() {
        let m = Mat3Q32::zero();
        assert_eq!(m.inverse(), None);
    }

    #[test]
    fn test_inverse_product() {
        // Test that m * m.inverse() = identity (approximately)
        let m = Mat3Q32::from_f32(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 2.0);
        let inv = m.inverse().unwrap();
        let product = m * inv;
        // Should be approximately identity
        assert!((product.get(0, 0).to_f32() - 1.0).abs() < 0.01);
        assert!((product.get(1, 1).to_f32() - 1.0).abs() < 0.01);
        assert!((product.get(2, 2).to_f32() - 1.0).abs() < 0.01);
        assert!((product.get(0, 1).to_f32() - 0.0).abs() < 0.01);
    }
}
