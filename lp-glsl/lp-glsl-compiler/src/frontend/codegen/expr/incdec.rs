//! Increment/decrement expression code generation
//!
//! Handles pre-increment (++i), pre-decrement (--i), post-increment (i++),
//! and post-decrement (i--) operations on scalars, vectors, matrices, and vector components.
//! Implements GLSL spec: operators.adoc:856-869

use crate::error::{ErrorCode, GlslError, source_span_to_location};
use crate::frontend::codegen::context::CodegenContext;
use crate::frontend::codegen::lvalue::{read_lvalue, resolve_lvalue, write_lvalue};
use crate::frontend::codegen::rvalue::RValue;
use crate::semantic::type_check::{
    infer_postdec_result_type, infer_postinc_result_type, infer_predec_result_type,
    infer_preinc_result_type,
};
use crate::semantic::types::Type as GlslType;
use cranelift_codegen::ir::{InstBuilder, Value, types};
use glsl::syntax::Expr;

use alloc::{format, vec::Vec};

/// Translate pre-increment expression (++i)
pub fn emit_preinc<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    operand: &Expr,
    span: glsl::syntax::SourceSpan,
) -> Result<(Vec<Value>, GlslType), GlslError> {
    emit_incdec(ctx, operand, span, true, false, true)
}

/// Translate pre-decrement expression (--i)
pub fn emit_predec<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    operand: &Expr,
    span: glsl::syntax::SourceSpan,
) -> Result<(Vec<Value>, GlslType), GlslError> {
    emit_incdec(ctx, operand, span, false, true, true)
}

/// Translate post-increment expression (i++)
pub fn emit_postinc<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    operand: &Expr,
    span: glsl::syntax::SourceSpan,
) -> Result<(Vec<Value>, GlslType), GlslError> {
    emit_incdec(ctx, operand, span, true, false, false)
}

/// Translate post-decrement expression (i--)
pub fn emit_postdec<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    operand: &Expr,
    span: glsl::syntax::SourceSpan,
) -> Result<(Vec<Value>, GlslType), GlslError> {
    emit_incdec(ctx, operand, span, false, true, false)
}

/// Common implementation for increment/decrement operations
fn emit_incdec<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    operand: &Expr,
    span: glsl::syntax::SourceSpan,
    is_increment: bool,
    is_decrement: bool,
    is_pre: bool,
) -> Result<(Vec<Value>, GlslType), GlslError> {
    // Resolve the operand to an LValue
    let lvalue = resolve_lvalue(ctx, operand)?;

    // Get the type of the LValue for type checking
    let lvalue_ty = lvalue.ty();

    // Type check the operation
    let result_ty = match (is_increment, is_decrement, is_pre) {
        (true, false, true) => infer_preinc_result_type(&lvalue_ty, span.clone())?,
        (false, true, true) => infer_predec_result_type(&lvalue_ty, span.clone())?,
        (true, false, false) => infer_postinc_result_type(&lvalue_ty, span.clone())?,
        (false, true, false) => infer_postdec_result_type(&lvalue_ty, span.clone())?,
        _ => unreachable!(),
    };

    // Read current values
    let (old_values, _) = read_lvalue(ctx, &lvalue)?;

    // Determine base type for arithmetic operations
    let base_ty = if lvalue_ty.is_matrix() {
        GlslType::Float // Matrices are always float
    } else if lvalue_ty.is_vector() {
        lvalue_ty.vector_base_type().unwrap()
    } else {
        lvalue_ty.clone()
    };

    // Compute new values (add or subtract 1 from each component)
    let mut new_values = Vec::new();
    for old_value in &old_values {
        let new_value = match base_ty {
            GlslType::Int => {
                let one = ctx
                    .builder
                    .ins()
                    .iconst(types::I32, if is_increment { 1 } else { -1 });
                ctx.builder.ins().iadd(*old_value, one)
            }
            GlslType::Float => {
                let one = ctx.builder.ins().f32const(1.0);
                if is_increment {
                    ctx.builder.ins().fadd(*old_value, one)
                } else if is_decrement {
                    ctx.builder.ins().fsub(*old_value, one)
                } else {
                    unreachable!()
                }
            }
            _ => {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    format!("increment/decrement not supported for type {base_ty:?}"),
                )
                .with_location(source_span_to_location(&span)));
            }
        };
        new_values.push(new_value);
    }

    // Write new values back
    write_lvalue(ctx, &lvalue, &new_values)?;

    // Return values based on pre vs post semantics
    let return_values = if is_pre {
        // Pre-increment/decrement: return new values
        new_values
    } else {
        // Post-increment/decrement: return old values
        old_values
    };

    Ok((return_values, result_ty))
}

/// Emit post-increment expression as RValue
///
/// Returns the original value before incrementing (post-increment semantics).
pub fn emit_postinc_rvalue<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    expr: &Expr,
) -> Result<RValue, GlslError> {
    let Expr::PostInc(operand, span) = expr else {
        unreachable!("emit_postinc_rvalue called on non-postinc expr");
    };
    let (vals, ty) = emit_postinc(ctx, operand, span.clone())?;
    Ok(RValue::from_aggregate(vals, ty))
}

/// Emit post-decrement expression as RValue
///
/// Returns the original value before decrementing (post-decrement semantics).
pub fn emit_postdec_rvalue<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    expr: &Expr,
) -> Result<RValue, GlslError> {
    let Expr::PostDec(operand, span) = expr else {
        unreachable!("emit_postdec_rvalue called on non-postdec expr");
    };
    let (vals, ty) = emit_postdec(ctx, operand, span.clone())?;
    Ok(RValue::from_aggregate(vals, ty))
}
