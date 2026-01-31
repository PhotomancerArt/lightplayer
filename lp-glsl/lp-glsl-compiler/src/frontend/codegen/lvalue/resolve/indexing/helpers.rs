//! Helper functions for indexing resolution

use crate::error::{ErrorCode, GlslError, extract_span_from_expr, source_span_to_location};
use crate::frontend::codegen::context::CodegenContext;
use crate::semantic::types::Type as GlslType;
use alloc::{format, vec::Vec};
use cranelift_frontend::Variable;
use glsl::syntax::{Expr, SourceSpan};

use super::super::super::read::read_lvalue;
use super::super::super::types::LValue;

/// Extract base variables and type from an LValue or expression
pub fn extract_base_vars_and_ty<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    base_expr: &Expr,
    span: &SourceSpan,
) -> Result<(Vec<Variable>, GlslType), GlslError> {
    use super::super::super::resolve_lvalue;

    if let Expr::Variable(ident, _) = base_expr {
        // We already handled array case above, so this must be a non-array variable
        let vars = ctx
            .lookup_variables(&ident.name)
            .ok_or_else(|| {
                let span = extract_span_from_expr(base_expr);
                let error = GlslError::undefined_variable(&ident.name)
                    .with_location(source_span_to_location(&span));
                ctx.add_span_to_error(error, &span)
            })?
            .to_vec();
        let ty = ctx.lookup_variable_type(&ident.name).unwrap().clone();
        Ok((vars, ty))
    } else {
        // Recursively resolve the base expression to an LValue (for matrices/vectors)
        let base_lvalue = resolve_lvalue(ctx, base_expr)?;

        // Get base variables and type
        match base_lvalue {
            LValue::Variable { vars, ty, .. } => Ok((vars, ty)),
            LValue::Component {
                base_vars, base_ty, ..
            } => Ok((base_vars, base_ty)),
            LValue::MatrixColumn {
                base_vars, base_ty, ..
            } => Ok((base_vars, base_ty)),
            LValue::MatrixElement {
                base_vars, base_ty, ..
            } => Ok((base_vars, base_ty)),
            LValue::VectorElement {
                base_vars, base_ty, ..
            } => Ok((base_vars, base_ty)),
            LValue::PointerBased { .. } => {
                // TODO: Handle PointerBased in Phase 2/3
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    "PointerBased LValue indexing not yet implemented",
                ));
            }
            LValue::ArrayElement { ref element_ty, .. } => {
                // Array element contains a matrix/vector - need to load it first
                // This handles cases like mats[0][0][0] where mats[0] is an ArrayElement
                if element_ty.is_matrix() || element_ty.is_vector() {
                    // Load the array element to get its values
                    let (vals, _) = read_lvalue(ctx, &base_lvalue)?;
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
                    Ok((vars, element_ty.clone()))
                } else {
                    Err(
                        GlslError::new(ErrorCode::E0400, "nested array indexing not yet supported")
                            .with_location(source_span_to_location(span)),
                    )
                }
            }
        }
    }
}

/// Validate index expression and extract compile-time constant index
pub fn validate_index(index_expr: &Expr, span: &SourceSpan) -> Result<usize, GlslError> {
    match index_expr {
        Expr::IntConst(n, _) => Ok(*n as usize),
        _ => Err(GlslError::new(
            ErrorCode::E0400,
            "variable-indexed writes not yet implemented",
        )
        .with_location(source_span_to_location(span))
        .with_note("only compile-time constant indices are supported for writes")),
    }
}

/// Process a matrix dimension index
pub fn process_matrix_dimension(
    current_ty: &GlslType,
    index: usize,
    span: &SourceSpan,
) -> Result<(GlslType, Option<usize>), GlslError> {
    let (_rows, cols) = current_ty.matrix_dims().unwrap();

    if index >= cols {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "matrix column index {} out of bounds (max {})",
                index,
                cols - 1
            ),
        )
        .with_location(source_span_to_location(span)));
    }

    let column_ty = current_ty.matrix_column_type().unwrap();
    Ok((column_ty, Some(index)))
}

/// Process a vector dimension index
pub fn process_vector_dimension(
    current_ty: &GlslType,
    index: usize,
    span: &SourceSpan,
) -> Result<GlslType, GlslError> {
    let component_count = current_ty.component_count().unwrap();

    if index >= component_count {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "vector component index {} out of bounds (max {})",
                index,
                component_count - 1
            ),
        )
        .with_location(source_span_to_location(span)));
    }

    Ok(current_ty.vector_base_type().unwrap())
}
