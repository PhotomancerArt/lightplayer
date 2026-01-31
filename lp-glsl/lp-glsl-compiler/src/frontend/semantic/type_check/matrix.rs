//! Matrix operation type inference
//! Implements GLSL spec: operators.adoc:1019-1098

use crate::error::{ErrorCode, GlslError, source_span_to_location};
use crate::frontend::semantic::types::Type;
use glsl::syntax::{BinaryOp, SourceSpan};

use alloc::format;

/// Infer result type of matrix binary operation
/// Implements GLSL spec: operators.adoc:1019-1098
pub fn infer_matrix_binary_result_type(
    op: &BinaryOp,
    lhs_ty: &Type,
    rhs_ty: &Type,
    span: SourceSpan,
) -> Result<Type, GlslError> {
    use BinaryOp::*;

    match op {
        // Matrix + Matrix: component-wise addition (same dimensions)
        Add => {
            if lhs_ty.is_matrix() && rhs_ty.is_matrix() {
                if lhs_ty != rhs_ty {
                    return Err(GlslError::new(
                        ErrorCode::E0106,
                        "matrix addition requires matching matrix types",
                    )
                    .with_location(source_span_to_location(&span))
                    .with_note(format!(
                        "left operand: `{lhs_ty:?}`, right operand: `{rhs_ty:?}`"
                    )));
                }
                return Ok(lhs_ty.clone());
            }
            Err(GlslError::new(
                ErrorCode::E0106,
                "matrix addition requires both operands to be matrices",
            )
            .with_location(source_span_to_location(&span)))
        }

        // Matrix - Matrix: component-wise subtraction (same dimensions)
        Sub => {
            if lhs_ty.is_matrix() && rhs_ty.is_matrix() {
                if lhs_ty != rhs_ty {
                    return Err(GlslError::new(
                        ErrorCode::E0106,
                        "matrix subtraction requires matching matrix types",
                    )
                    .with_location(source_span_to_location(&span))
                    .with_note(format!(
                        "left operand: `{lhs_ty:?}`, right operand: `{rhs_ty:?}`"
                    )));
                }
                return Ok(lhs_ty.clone());
            }
            Err(GlslError::new(
                ErrorCode::E0106,
                "matrix subtraction requires both operands to be matrices",
            )
            .with_location(source_span_to_location(&span)))
        }

        // Matrix multiplication
        Mult => {
            // Matrix × Scalar: component-wise multiplication
            if lhs_ty.is_matrix() && rhs_ty.is_scalar() {
                if !rhs_ty.is_numeric() {
                    return Err(GlslError::new(
                        ErrorCode::E0106,
                        "matrix × scalar requires numeric scalar",
                    )
                    .with_location(source_span_to_location(&span)));
                }
                return Ok(lhs_ty.clone());
            }

            // Scalar × Matrix: component-wise multiplication
            if lhs_ty.is_scalar() && rhs_ty.is_matrix() {
                if !lhs_ty.is_numeric() {
                    return Err(GlslError::new(
                        ErrorCode::E0106,
                        "scalar × matrix requires numeric scalar",
                    )
                    .with_location(source_span_to_location(&span)));
                }
                return Ok(rhs_ty.clone());
            }

            // Matrix × Vector: linear algebra multiplication
            if lhs_ty.is_matrix() && rhs_ty.is_vector() {
                let (rows, cols) = lhs_ty.matrix_dims().unwrap();
                let vec_size = rhs_ty.component_count().unwrap();

                if cols != vec_size {
                    return Err(GlslError::new(
                        ErrorCode::E0106,
                        format!("matrix × vector dimension mismatch: {rows}×{cols} matrix requires {cols}-component vector")
                    )
                    .with_location(source_span_to_location(&span))
                    .with_note(format!("got {vec_size}-component vector")));
                }
                // Result is a vector with same number of components as matrix rows
                return Ok(lhs_ty.matrix_column_type().unwrap());
            }

            // Vector × Matrix: linear algebra multiplication
            if lhs_ty.is_vector() && rhs_ty.is_matrix() {
                let vec_size = lhs_ty.component_count().unwrap();
                let (rows, cols) = rhs_ty.matrix_dims().unwrap();

                if vec_size != rows {
                    return Err(GlslError::new(
                        ErrorCode::E0106,
                        format!("vector × matrix dimension mismatch: {vec_size}-component vector requires {rows}×{cols} matrix")
                    )
                    .with_location(source_span_to_location(&span))
                    .with_note(format!("got {rows}×{cols} matrix")));
                }
                // Result is a vector with same number of components as matrix columns
                // For vec3 × mat3, result is vec3 (but conceptually row vector)
                // GLSL treats this as returning a column vector
                return Ok(Type::vector_type(&Type::Float, cols).unwrap());
            }

            // Matrix × Matrix: linear algebra multiplication
            if lhs_ty.is_matrix() && rhs_ty.is_matrix() {
                let (lhs_rows, lhs_cols) = lhs_ty.matrix_dims().unwrap();
                let (rhs_rows, rhs_cols) = rhs_ty.matrix_dims().unwrap();

                if lhs_cols != rhs_rows {
                    return Err(GlslError::new(
                        ErrorCode::E0106,
                        format!(
                            "matrix × matrix dimension mismatch: {lhs_rows}×{lhs_cols} × {rhs_rows}×{rhs_cols} requires {lhs_cols} == {rhs_rows}"
                        ),
                    )
                    .with_location(source_span_to_location(&span)));
                }
                // Result is lhs_rows × rhs_cols matrix
                // For now, we only support square matrices, so result type matches lhs
                if lhs_rows == rhs_cols {
                    return Ok(lhs_ty.clone());
                }
                // Non-square not yet supported
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    "non-square matrix multiplication not yet supported",
                )
                .with_location(source_span_to_location(&span)));
            }

            Err(GlslError::new(
                ErrorCode::E0106,
                "matrix multiplication requires matrix and scalar/vector/matrix operands",
            )
            .with_location(source_span_to_location(&span)))
        }

        Div => {
            // Matrix / Scalar: component-wise division
            if lhs_ty.is_matrix() && rhs_ty.is_scalar() {
                if !rhs_ty.is_numeric() {
                    return Err(GlslError::new(
                        ErrorCode::E0106,
                        "matrix / scalar requires numeric scalar",
                    )
                    .with_location(source_span_to_location(&span)));
                }
                return Ok(lhs_ty.clone());
            }
            Err(GlslError::new(
                ErrorCode::E0106,
                "matrix division only supports matrix / scalar",
            )
            .with_location(source_span_to_location(&span)))
        }

        // Matrix == Matrix and Matrix != Matrix: aggregate comparison
        Equal | NonEqual => {
            if lhs_ty.is_matrix() && rhs_ty.is_matrix() {
                if lhs_ty != rhs_ty {
                    return Err(GlslError::new(
                        ErrorCode::E0106,
                        "matrix equality requires matching matrix types",
                    )
                    .with_location(source_span_to_location(&span))
                    .with_note(format!(
                        "left operand: `{lhs_ty:?}`, right operand: `{rhs_ty:?}`"
                    )));
                }
                return Ok(Type::Bool);
            }
            Err(GlslError::new(
                ErrorCode::E0106,
                "matrix equality requires both operands to be matrices",
            )
            .with_location(source_span_to_location(&span)))
        }

        _ => Err(GlslError::new(
            ErrorCode::E0106,
            format!("operator {op:?} not supported for matrices"),
        )
        .with_location(source_span_to_location(&span))),
    }
}
