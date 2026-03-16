//! Assignment expression code generation

use crate::error::{ErrorCode, GlslError, source_span_to_location};
use crate::frontend::codegen::context::CodegenContext;
use crate::frontend::codegen::lvalue::write_lvalue;
use crate::frontend::codegen::rvalue::RValue;
use crate::semantic::types::Type as GlslType;
use cranelift_codegen::ir::Value;
use glsl::syntax::Expr;

use alloc::{format, vec, vec::Vec};

/// Emit assignment expression as RValue
///
/// Evaluates an assignment expression and returns the assigned value(s) as RValue.
pub fn emit_assignment_rvalue<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    expr: &Expr,
) -> Result<RValue, GlslError> {
    let Expr::Assignment(lhs, op, rhs, _span) = expr else {
        unreachable!("emit_assignment_rvalue called on non-assignment expr");
    };
    let (vals, ty) = emit_assignment_typed(ctx, lhs, op, rhs)?;
    Ok(RValue::from_aggregate(vals, ty))
}

/// Translate assignment expression (simple and compound)
///
/// Handles both simple assignment (=) and compound assignment (+=, -=, *=, /=).
/// Returns the assigned value(s) and type.
pub fn emit_assignment_typed<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    lhs: &Expr,
    op: &glsl::syntax::AssignmentOp,
    rhs: &Expr,
) -> Result<(Vec<Value>, GlslType), GlslError> {
    use super::component;
    use crate::error::extract_span_from_expr;
    use crate::error::extract_span_from_identifier;
    use crate::frontend::codegen::lvalue::resolve_lvalue;
    use crate::semantic::type_check::conversion::check_assignment;

    // Handle compound assignment operators (+=, -=, *=, /=)
    if !matches!(op, glsl::syntax::AssignmentOp::Equal) {
        return emit_compound_assignment_typed(ctx, lhs, op, rhs);
    }

    // Resolve LHS to an LValue
    let lvalue = resolve_lvalue(ctx, lhs)?;
    let lhs_ty = lvalue.ty();

    // Special handling for component assignment with swizzles (check for duplicates)
    if let crate::frontend::codegen::lvalue::LValue::Component { indices, .. } = &lvalue {
        // Check for duplicates (illegal in assignment LHS)
        if component::has_duplicates(indices) {
            // Try to extract span from the field identifier
            if let Expr::Dot(_, field, _) = lhs {
                let span = extract_span_from_identifier(field);
                let error = GlslError::new(
                    ErrorCode::E0113,
                    format!(
                        "swizzle `{}` contains duplicate components (illegal in assignment)",
                        field.name
                    ),
                )
                .with_location(source_span_to_location(&span));
                return Err(ctx.add_span_to_error(error, &span));
            }
        }
    }

    // Translate RHS as RValue
    let rhs_rvalue = ctx.emit_rvalue(rhs)?;
    let rhs_ty = rhs_rvalue.ty().clone();
    let rhs_vals = rhs_rvalue.into_values();

    // Validate assignment (check implicit conversion is allowed)
    let rhs_span = extract_span_from_expr(rhs);
    match check_assignment(&lhs_ty, &rhs_ty) {
        Ok(()) => {}
        Err(mut error) => {
            if error.location.is_none() {
                error = error.with_location(source_span_to_location(&rhs_span));
            }
            return Err(ctx.add_span_to_error(error, &rhs_span));
        }
    }

    // Check component counts match
    let expected_count = match &lvalue {
        crate::frontend::codegen::lvalue::LValue::Variable { vars, .. } => vars.len(),
        crate::frontend::codegen::lvalue::LValue::Component { indices, .. } => indices.len(),
        crate::frontend::codegen::lvalue::LValue::MatrixElement { .. } => 1,
        crate::frontend::codegen::lvalue::LValue::MatrixColumn { result_ty, .. } => {
            result_ty.component_count().unwrap()
        }
        crate::frontend::codegen::lvalue::LValue::VectorElement { .. } => 1,
        crate::frontend::codegen::lvalue::LValue::ArrayElement {
            element_ty,
            component_indices,
            ..
        } => {
            if let Some(indices) = component_indices {
                indices.len()
            } else if element_ty.is_scalar() {
                1
            } else if element_ty.is_vector() {
                element_ty.component_count().unwrap()
            } else if element_ty.is_matrix() {
                element_ty.matrix_element_count().unwrap()
            } else {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    format!("unsupported array element type: {element_ty:?}"),
                ));
            }
        }
        crate::frontend::codegen::lvalue::LValue::PointerBased { access_pattern, .. } => {
            match access_pattern {
                crate::frontend::codegen::lvalue::PointerAccessPattern::Direct {
                    component_count,
                } => *component_count,
                crate::frontend::codegen::lvalue::PointerAccessPattern::Component {
                    indices,
                    ..
                } => indices.len(),
                crate::frontend::codegen::lvalue::PointerAccessPattern::ArrayElement {
                    element_ty,
                    component_indices,
                    ..
                } => {
                    if let Some(indices) = component_indices {
                        indices.len()
                    } else if element_ty.is_scalar() {
                        1
                    } else if element_ty.is_vector() {
                        element_ty.component_count().unwrap()
                    } else if element_ty.is_matrix() {
                        element_ty.matrix_element_count().unwrap()
                    } else {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            format!("unsupported array element type: {element_ty:?}"),
                        ));
                    }
                }
            }
        }
    };

    if expected_count != rhs_vals.len() {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "component count mismatch in assignment: {} vs {}",
                expected_count,
                rhs_vals.len()
            ),
        )
        .with_location(source_span_to_location(&rhs_span)));
    }

    // Coerce and assign each component
    let rhs_base = if rhs_ty.is_vector() {
        rhs_ty.vector_base_type().unwrap()
    } else if rhs_ty.is_matrix() {
        GlslType::Float // Matrix elements are always float
    } else {
        rhs_ty.clone()
    };
    let lhs_base = if lhs_ty.is_vector() {
        lhs_ty.vector_base_type().unwrap()
    } else if lhs_ty.is_matrix() {
        GlslType::Float // Matrix elements are always float
    } else {
        lhs_ty.clone()
    };

    let mut coerced_vals = Vec::new();
    for val in &rhs_vals {
        let coerced = super::coercion::coerce_to_type(ctx, *val, &rhs_base, &lhs_base)?;
        coerced_vals.push(coerced);
    }

    // Write coerced values to LValue
    write_lvalue(ctx, &lvalue, &coerced_vals)?;

    // For component assignment, return all current values (read other components)
    // For other assignments, return the assigned values
    let result_vals = match &lvalue {
        crate::frontend::codegen::lvalue::LValue::Component {
            base_vars, base_ty, ..
        } => {
            // Component assignment returns the whole vector/matrix
            let mut result_vals = Vec::new();
            for &var in base_vars {
                result_vals.push(ctx.builder.use_var(var));
            }
            (result_vals, base_ty.clone())
        }
        _ => {
            // Other assignments return the assigned values
            (coerced_vals, lhs_ty)
        }
    };

    Ok(result_vals)
}

/// Handle compound assignment operators (+=, -=, *=, /=)
fn emit_compound_assignment_typed<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    lhs: &Expr,
    op: &glsl::syntax::AssignmentOp,
    rhs: &Expr,
) -> Result<(Vec<Value>, GlslType), GlslError> {
    use super::binary;
    use super::matrix;
    use super::vector;
    use crate::error::extract_span_from_expr;
    use crate::frontend::codegen::lvalue::{read_lvalue, resolve_lvalue, write_lvalue};
    use crate::semantic::type_check::conversion::check_assignment;
    use glsl::syntax::BinaryOp;

    // Resolve LHS to an LValue
    let lvalue = resolve_lvalue(ctx, lhs)?;
    let lhs_ty = lvalue.ty();

    // Translate RHS as RValue
    let rhs_rvalue = ctx.emit_rvalue(rhs)?;
    let rhs_ty = rhs_rvalue.ty().clone();
    let rhs_vals = rhs_rvalue.into_values();
    let rhs_span = extract_span_from_expr(rhs);

    // Validate assignment types
    // For compound assignment, we allow:
    // - Same type operations (matrix + matrix, vector + vector, scalar + scalar)
    // - Scalar operations on matrices/vectors (matrix * scalar, vector * scalar)
    // Only validate direct assignment compatibility for same-type operations
    let is_scalar_op_on_matrix = (lhs_ty.is_matrix() || lhs_ty.is_vector()) && rhs_ty.is_scalar();
    let is_scalar_op_on_vector = lhs_ty.is_vector() && rhs_ty.is_scalar();

    if !is_scalar_op_on_matrix && !is_scalar_op_on_vector {
        // For same-type operations, validate assignment compatibility
        match check_assignment(&lhs_ty, &rhs_ty) {
            Ok(()) => {}
            Err(mut error) => {
                if error.location.is_none() {
                    error = error.with_location(source_span_to_location(&rhs_span));
                }
                return Err(ctx.add_span_to_error(error, &rhs_span));
            }
        }
    }

    // Read current value from LHS
    let (lhs_vals, _) = read_lvalue(ctx, &lvalue)?;

    // Convert assignment operator to binary operator
    let binary_op = match op {
        glsl::syntax::AssignmentOp::Add => BinaryOp::Add,
        glsl::syntax::AssignmentOp::Sub => BinaryOp::Sub,
        glsl::syntax::AssignmentOp::Mult => BinaryOp::Mult,
        glsl::syntax::AssignmentOp::Div => BinaryOp::Div,
        _ => {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!("unsupported compound assignment operator: {op:?}"),
            ));
        }
    };

    // Perform the compound operation
    let (operation_result_vals, operation_result_ty) = if lhs_ty.is_matrix() || rhs_ty.is_matrix() {
        // Use matrix operations for matrix compound assignments
        matrix::emit_matrix_binary(
            ctx,
            &binary_op,
            lhs_vals,
            &lhs_ty,
            rhs_vals,
            &rhs_ty,
            rhs_span.clone(),
        )?
    } else if lhs_ty.is_vector() || rhs_ty.is_vector() {
        // Use vector operations
        vector::emit_vector_binary(
            ctx,
            &binary_op,
            lhs_vals,
            &lhs_ty,
            rhs_vals,
            &rhs_ty,
            Some(rhs_span.clone()),
        )?
    } else {
        // Use scalar operations - need to determine base type for coercion
        let base_ty = if lhs_ty.is_numeric() && rhs_ty.is_numeric() {
            use crate::semantic::type_check::conversion::promote_numeric;
            promote_numeric(&lhs_ty, &rhs_ty)
        } else {
            lhs_ty.clone()
        };

        // Coerce operands to common type
        let lhs_val_coerced = super::coercion::coerce_to_type(ctx, lhs_vals[0], &lhs_ty, &base_ty)?;
        let rhs_val_coerced = super::coercion::coerce_to_type(ctx, rhs_vals[0], &rhs_ty, &base_ty)?;

        // Perform scalar operation
        let result_val = binary::emit_scalar_binary_op_internal(
            ctx,
            &binary_op,
            lhs_val_coerced,
            rhs_val_coerced,
            &base_ty,
            rhs_span.clone(),
        )?;

        // Result type is the same as the promoted type
        (vec![result_val], base_ty)
    };

    // Write result back to LHS
    write_lvalue(ctx, &lvalue, &operation_result_vals)?;

    // Return the result (same as simple assignment)
    let final_result = match &lvalue {
        crate::frontend::codegen::lvalue::LValue::Component {
            base_vars, base_ty, ..
        } => {
            // Component assignment returns the whole vector/matrix
            let mut result_vals = Vec::new();
            for &var in base_vars {
                result_vals.push(ctx.builder.use_var(var));
            }
            (result_vals, base_ty.clone())
        }
        _ => {
            // Other assignments return the assigned values
            (operation_result_vals, operation_result_ty)
        }
    };

    Ok(final_result)
}
