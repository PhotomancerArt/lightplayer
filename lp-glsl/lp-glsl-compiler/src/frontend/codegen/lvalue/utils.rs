//! Utility functions for LValue operations

use crate::semantic::types::Type as GlslType;
use alloc::vec::Vec;

/// Compute matrix variable indices for column components
///
/// When accessing components of a matrix column (e.g., `m[0].x`), we need to map
/// the component indices (0=x, 1=y, etc.) to the correct matrix variable indices.
/// For a column `col` and component index `comp_idx`, the matrix variable index is `col * rows + comp_idx`.
pub fn compute_column_variable_indices(
    base_ty: &GlslType,
    col: usize,
    component_indices: &[usize],
) -> Vec<usize> {
    let (rows, _cols) = base_ty.matrix_dims().unwrap();
    component_indices
        .iter()
        .map(|&comp_idx| col * rows + comp_idx)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_column_variable_indices() {
        // Test mat2 (2x2 matrix)
        // Column 0: indices 0, 1
        // Column 1: indices 2, 3
        let mat2_ty = GlslType::Mat2;
        let indices = compute_column_variable_indices(&mat2_ty, 0, &[0, 1]);
        assert_eq!(indices, vec![0, 1]);

        let indices = compute_column_variable_indices(&mat2_ty, 1, &[0, 1]);
        assert_eq!(indices, vec![2, 3]);

        // Test mat3 (3x3 matrix)
        // Column 0: indices 0, 1, 2
        // Column 1: indices 3, 4, 5
        // Column 2: indices 6, 7, 8
        let mat3_ty = GlslType::Mat3;
        let indices = compute_column_variable_indices(&mat3_ty, 1, &[0, 2]);
        assert_eq!(indices, vec![3, 5]);
    }
}
