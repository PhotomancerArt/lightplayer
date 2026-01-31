//! Component access on MatrixColumn LValue

use crate::semantic::types::Type as GlslType;
use alloc::vec::Vec;
use cranelift_frontend::Variable;

use super::super::super::types::LValue;
use super::super::super::utils::compute_column_variable_indices;

/// Resolve component access on a MatrixColumn LValue
pub fn resolve_component_on_matrix_column(
    base_vars: Vec<Variable>,
    matrix_ty: GlslType,
    col: usize,
    indices: Vec<usize>,
    result_ty: GlslType,
) -> LValue {
    // When accessing components of a matrix column, we need to map component indices
    // (0=x, 1=y, etc.) to the correct matrix variable indices.
    // For column `col` and component index `comp_idx`, the matrix variable index is `col * rows + comp_idx`.
    let matrix_indices = compute_column_variable_indices(&matrix_ty, col, &indices);
    LValue::Component {
        base_vars,
        base_ty: matrix_ty,
        indices: matrix_indices,
        result_ty,
    }
}
