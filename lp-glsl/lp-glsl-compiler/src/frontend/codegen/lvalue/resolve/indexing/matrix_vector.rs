//! Matrix and vector indexing resolution

use crate::error::{ErrorCode, GlslError, source_span_to_location};
use crate::frontend::codegen::context::CodegenContext;
use crate::semantic::types::Type as GlslType;
use glsl::syntax::{ArraySpecifier, ArraySpecifierDimension, SourceSpan};

use alloc::format;

use super::super::super::types::LValue;
use super::helpers::{
    extract_base_vars_and_ty, process_matrix_dimension, process_vector_dimension, validate_index,
};

/// Resolve matrix/vector indexing (e.g., m[0], m[0][1], v[0])
pub fn resolve_matrix_vector_indexing<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    base_expr: &glsl::syntax::Expr,
    array_spec: &ArraySpecifier,
    span: &SourceSpan,
) -> Result<LValue, GlslError> {
    let (base_vars, base_ty) = extract_base_vars_and_ty(ctx, base_expr, span)?;

    if array_spec.dimensions.0.is_empty() {
        return Err(
            GlslError::new(ErrorCode::E0400, "indexing requires explicit index")
                .with_location(source_span_to_location(span)),
        );
    }

    // Process dimensions one at a time
    let mut current_ty = base_ty.clone();
    let current_vars = base_vars;
    let mut row: Option<usize> = None;
    let mut col: Option<usize> = None;

    for (_dim_idx, dimension) in array_spec.dimensions.0.iter().enumerate() {
        let index_expr = match dimension {
            ArraySpecifierDimension::ExplicitlySized(expr) => expr,
            ArraySpecifierDimension::Unsized => {
                return Err(
                    GlslError::new(ErrorCode::E0400, "indexing requires explicit index")
                        .with_location(source_span_to_location(span)),
                );
            }
        };

        // Evaluate index (must be int)
        let (_, index_ty) = ctx.emit_expr_typed(index_expr)?;
        if index_ty != GlslType::Int {
            return Err(GlslError::new(ErrorCode::E0106, "index must be int")
                .with_location(source_span_to_location(span)));
        }

        // Extract compile-time constant index
        // For LValues (writes), we only support constant indices
        // Variable-indexed reads are handled via translate_matrix_indexing()
        let index = validate_index(index_expr, span)?;

        if current_ty.is_matrix() {
            // Matrix indexing: mat[col] returns column vector
            let (new_ty, col_idx) = process_matrix_dimension(&current_ty, index, span)?;
            col = col_idx;
            current_ty = new_ty;
            // Don't update current_vars here - we'll use them for the final LValue
        } else if current_ty.is_vector() {
            // Vector indexing: vec[index] returns scalar component
            // If we already have a column, this is a matrix element access
            if col.is_some() {
                row = Some(index);
                current_ty = process_vector_dimension(&current_ty, index, span)?;
            } else {
                // This is vector element access: v[0] -> scalar
                return Ok(LValue::VectorElement {
                    base_vars: current_vars,
                    base_ty: base_ty.clone(),
                    index,
                });
            }
        } else if current_ty.is_array() {
            return Err(GlslError::new(
                ErrorCode::E0400,
                "multi-dimensional array indexing not yet supported",
            )
            .with_location(source_span_to_location(span)));
        } else {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "cannot index into {current_ty:?} (only arrays, matrices and vectors can be indexed)"
                ),
            )
            .with_location(source_span_to_location(span)));
        }
    }

    // Determine the final LValue type based on what we found
    match (row, col) {
        (Some(row), Some(col)) => {
            // Matrix element: m[col][row]
            Ok(LValue::MatrixElement {
                base_vars: current_vars,
                base_ty: base_ty.clone(),
                row,
                col,
            })
        }
        (None, Some(col)) => {
            // Matrix column: m[col]
            let column_ty = base_ty.matrix_column_type().unwrap();
            Ok(LValue::MatrixColumn {
                base_vars: current_vars,
                base_ty: base_ty.clone(),
                col,
                result_ty: column_ty,
            })
        }
        _ => {
            // Shouldn't happen, but handle gracefully
            Err(GlslError::new(ErrorCode::E0400, "invalid indexing pattern")
                .with_location(source_span_to_location(span)))
        }
    }
}
