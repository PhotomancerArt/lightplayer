//! Nested array indexing resolution (arrays of matrices/vectors)

use crate::error::{ErrorCode, GlslError, source_span_to_location};
use crate::frontend::codegen::context::CodegenContext;
use crate::semantic::types::Type as GlslType;
use alloc::{format, vec::Vec};
use cranelift_codegen::ir::Value;
use glsl::syntax::{ArraySpecifier, ArraySpecifierDimension, SourceSpan};

use super::super::super::read::read_lvalue;
use super::super::super::types::LValue;
use super::helpers::{process_matrix_dimension, process_vector_dimension, validate_index};

/// Resolve nested array indexing (e.g., arr[i][0], arr[i][0][1])
pub fn resolve_nested_array_indexing<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    array_ptr: Value,
    base_ty: GlslType,
    compile_time_index: Option<usize>,
    index_val: Option<Value>,
    element_ty: GlslType,
    element_size_bytes: usize,
    array_spec: &ArraySpecifier,
    span: &SourceSpan,
) -> Result<LValue, GlslError> {
    // Load the array element to get its values
    let array_element_lvalue = LValue::ArrayElement {
        array_ptr,
        base_ty: base_ty.clone(),
        index: compile_time_index,
        index_val,
        element_ty: element_ty.clone(),
        element_size_bytes,
        component_indices: None,
    };
    let (vals, _) = read_lvalue(ctx, &array_element_lvalue)?;

    // Create temporary variables to hold the loaded values
    let base_cranelift_ty = if element_ty.is_vector() {
        let base_ty = element_ty.vector_base_type().unwrap();
        base_ty.to_cranelift_type().map_err(|e| {
            GlslError::new(
                ErrorCode::E0400,
                format!("Failed to convert vector base type: {}", e.message),
            )
        })?
    } else {
        // Matrix - always float
        cranelift_codegen::ir::types::F32
    };
    let mut vars = Vec::new();
    for val in vals {
        let var = ctx.builder.declare_var(base_cranelift_ty);
        ctx.builder.def_var(var, val);
        vars.push(var);
    }

    // Continue processing remaining dimensions with the loaded matrix/vector
    let base_vars = vars;
    let base_ty = element_ty;

    // Process remaining dimensions (skip the first one we already handled)
    if array_spec.dimensions.0.len() <= 1 {
        return Err(GlslError::new(
            ErrorCode::E0400,
            "expected more dimensions after array index",
        )
        .with_location(source_span_to_location(span)));
    }

    // Process dimensions starting from index 1 (skip the first array dimension)
    let mut current_ty = base_ty.clone();
    let current_vars = base_vars;
    let mut col: Option<usize> = None;

    log::trace!(
        "Processing nested dimensions: base_ty={:?}, remaining_dims={}",
        base_ty,
        array_spec.dimensions.0.len() - 1
    );

    for (_dim_idx, dimension) in array_spec.dimensions.0.iter().skip(1).enumerate() {
        log::trace!(
            "  Processing dimension {}: current_ty={:?}, col={:?}",
            _dim_idx + 1,
            current_ty,
            col
        );
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
        let index = validate_index(index_expr, span)?;

        if current_ty.is_matrix() {
            // Matrix indexing: mat[col] returns column vector
            let (new_ty, col_idx) = process_matrix_dimension(&current_ty, index, span)?;
            col = col_idx;
            current_ty = new_ty;
        } else if current_ty.is_vector() {
            // Vector indexing: vec[index] returns scalar component
            // If we already have a column, this is a matrix element access
            if col.is_some() {
                let row = index;
                log::trace!(
                    "  Matrix element access: col={}, row={}, base_ty={:?}",
                    col.unwrap(),
                    row,
                    base_ty
                );
                let _ = process_vector_dimension(&current_ty, index, span)?;
                return Ok(LValue::MatrixElement {
                    base_vars: current_vars,
                    base_ty: base_ty.clone(),
                    row,
                    col: col.unwrap(),
                });
            } else {
                log::trace!("  Vector element access: index={index}");
                // This is vector element access: v[0] -> scalar
                return Ok(LValue::VectorElement {
                    base_vars: current_vars,
                    base_ty: base_ty.clone(),
                    index,
                });
            }
        } else {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "cannot index into {current_ty:?} (only matrices and vectors can be indexed after array)"
                ),
            )
            .with_location(source_span_to_location(span)));
        }
    }

    // If we get here, we processed all dimensions but didn't return
    // This means we have a matrix column
    if let Some(col_idx) = col {
        Ok(LValue::MatrixColumn {
            base_vars: current_vars,
            base_ty: base_ty.clone(),
            col: col_idx,
            result_ty: current_ty,
        })
    } else {
        Err(
            GlslError::new(ErrorCode::E0400, "unexpected state in array indexing")
                .with_location(source_span_to_location(span)),
        )
    }
}
