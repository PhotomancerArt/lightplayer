use glsl::syntax::IterationStatement;

use crate::error::{ErrorCode, GlslError};
use crate::frontend::codegen::context::CodegenContext;

use alloc::format;

/// Emit iteration statement (dispatch to specific loop types)
pub fn emit_iteration_stmt<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    iteration: &IterationStatement,
) -> Result<(), GlslError> {
    use glsl::syntax::IterationStatement;

    match iteration {
        IterationStatement::While(condition, body) => {
            crate::frontend::codegen::stmt::loop_while::emit_loop_while_stmt(ctx, condition, body)
        }
        IterationStatement::DoWhile(body, expr) => {
            crate::frontend::codegen::stmt::loop_do_while::emit_loop_do_while_stmt(ctx, body, expr)
        }
        IterationStatement::For(init, rest, body) => {
            crate::frontend::codegen::stmt::loop_for::emit_loop_for_stmt(ctx, init, rest, body)
        }
    }
}

/// Shared helper: translate condition expression to boolean value
/// Supports both expression conditions and variable declaration conditions (e.g., `while (bool j = i < 3)`)
pub fn emit_condition<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    condition: &glsl::syntax::Condition,
) -> Result<cranelift_codegen::ir::Value, GlslError> {
    use crate::frontend::codegen::stmt::declaration::{emit_initializer, parse_type_specifier};

    match condition {
        glsl::syntax::Condition::Expr(expr) => {
            let (vals, ty) = ctx.emit_expr_typed(expr)?;
            // Validate that condition is bool type (GLSL spec requirement)
            let cond_span = crate::error::extract_span_from_expr(expr);
            match crate::frontend::semantic::type_check::check_condition(&ty) {
                Ok(()) => {}
                Err(mut error) => {
                    if error.location.is_none() {
                        error =
                            error.with_location(crate::error::source_span_to_location(&cond_span));
                    }
                    return Err(ctx.add_span_to_error(error, &cond_span));
                }
            }
            // Condition must be scalar, so we take the first (and only) value
            Ok(vals.into_iter().next().ok_or_else(|| {
                let error =
                    GlslError::new(ErrorCode::E0400, "condition expression produced no value")
                        .with_location(crate::error::source_span_to_location(&cond_span));
                ctx.add_span_to_error(error, &cond_span)
            })?)
        }
        glsl::syntax::Condition::Assignment(type_spec, identifier, initializer) => {
            // Variable declaration in condition: `while (bool j = i < 3)`
            // According to GLSL spec, the variable is declared, initialized, and its value is used as the condition

            // Parse the type
            let var_ty = parse_type_specifier(ctx, type_spec)?;

            // Declare the variable in the current scope
            let vars = ctx.declare_variable(identifier.name.clone(), var_ty.clone())?;

            // Evaluate the initializer
            let (init_vals, init_ty) = emit_initializer(ctx, initializer)?;

            // Extract span from initializer for error reporting
            let init_span = match initializer {
                glsl::syntax::Initializer::Simple(expr) => {
                    crate::error::extract_span_from_expr(expr.as_ref())
                }
                _ => glsl::syntax::SourceSpan::unknown(),
            };

            // Type check (allows implicit conversions)
            match crate::frontend::semantic::type_check::check_assignment(&var_ty, &init_ty) {
                Ok(()) => {}
                Err(mut error) => {
                    if error.location.is_none() {
                        error =
                            error.with_location(crate::error::source_span_to_location(&init_span));
                    }
                    return Err(ctx.add_span_to_error(error, &init_span));
                }
            }

            // Coerce initializer values to match variable type
            let base_ty = if var_ty.is_vector() {
                var_ty.vector_base_type().unwrap()
            } else if var_ty.is_matrix() {
                crate::frontend::semantic::types::Type::Float
            } else {
                var_ty.clone()
            };
            let init_base = if init_ty.is_vector() {
                init_ty.vector_base_type().unwrap()
            } else if init_ty.is_matrix() {
                crate::frontend::semantic::types::Type::Float
            } else {
                init_ty.clone()
            };

            // Check component counts match
            if vars.len() != init_vals.len() {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    format!(
                        "component count mismatch: variable has {} components, initializer has {}",
                        vars.len(),
                        init_vals.len()
                    ),
                ));
            }

            // Assign each component with type coercion
            for (var, val) in vars.iter().zip(&init_vals) {
                let coerced_val = ctx.coerce_to_type(*val, &init_base, &base_ty)?;
                ctx.builder.def_var(*var, coerced_val);
            }

            // Read the variable's value to use as the condition
            // Validate that the variable type is bool (GLSL spec requirement for conditions)
            // Use the initializer span for error reporting - it covers the expression being assigned
            // The error is about the whole condition expression, so we'll use init_span which covers
            // the right-hand side, and the formatting will extend the caret to cover the whole assignment
            match crate::frontend::semantic::type_check::check_condition(&var_ty) {
                Ok(()) => {}
                Err(mut error) => {
                    if error.location.is_none() {
                        error =
                            error.with_location(crate::error::source_span_to_location(&init_span));
                    }
                    // Use init_span but we need to create a span that covers the whole assignment
                    // For now, use init_span - the formatting function will need to handle extending it
                    return Err(ctx.add_span_to_error(error, &init_span));
                }
            }

            // Return the first (and only) component value
            Ok(ctx.builder.use_var(vars[0]))
        }
    }
}
