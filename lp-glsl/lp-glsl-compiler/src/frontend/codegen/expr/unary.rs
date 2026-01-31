use crate::error::{ErrorCode, GlslError};
use crate::frontend::codegen::context::CodegenContext;
use crate::frontend::codegen::rvalue::RValue;
use crate::semantic::type_check::operators::infer_unary_result_type;
use crate::semantic::types::Type as GlslType;
use cranelift_codegen::ir::{InstBuilder, Value, condcodes::IntCC, types};
use glsl::syntax::Expr;

use super::incdec;

use alloc::{format, vec::Vec};

/// Emit unary expression as RValue
///
/// Handles pre-increment/decrement specially, delegates other unary operations
/// to translate_unary_op.
pub fn emit_unary_rvalue<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    expr: &Expr,
) -> Result<RValue, GlslError> {
    let Expr::Unary(op, operand, span) = expr else {
        unreachable!("emit_unary_rvalue called on non-unary expr");
    };

    use glsl::syntax::UnaryOp::*;
    match op {
        Inc => {
            let (vals, ty) = incdec::emit_preinc(ctx, operand, span.clone())?;
            Ok(RValue::from_aggregate(vals, ty))
        }
        Dec => {
            let (vals, ty) = incdec::emit_predec(ctx, operand, span.clone())?;
            Ok(RValue::from_aggregate(vals, ty))
        }
        _ => {
            // Ensure we're in a block before evaluating
            ctx.ensure_block()?;

            let operand_rvalue = ctx.emit_rvalue(operand)?;
            let ty = operand_rvalue.ty().clone();
            let vals = operand_rvalue.into_values();

            let result_ty = infer_unary_result_type(op, &ty, span.clone())?;

            // Handle scalar, vector, and matrix operations
            if vals.len() == 1 {
                // Scalar operation
                let val = vals[0];
                let result_val = emit_unary_op(ctx, op, val, &ty)?;
                Ok(RValue::from_scalar(result_val, result_ty))
            } else {
                // Vector or matrix operation - apply component-wise
                let mut result_vals = Vec::new();
                for val in vals {
                    let result_val = emit_unary_op(ctx, op, val, &ty)?;
                    result_vals.push(result_val);
                }
                Ok(RValue::from_aggregate(result_vals, result_ty))
            }
        }
    }
}

/// TODO Legacy function for backwards compatibility
pub fn emit_unary<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    expr: &Expr,
) -> Result<(Vec<Value>, GlslType), GlslError> {
    let rvalue = emit_unary_rvalue(ctx, expr)?;
    let ty = rvalue.ty().clone();
    Ok((rvalue.into_values(), ty))
}

fn emit_unary_op<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    op: &glsl::syntax::UnaryOp,
    val: Value,
    operand_ty: &GlslType,
) -> Result<Value, GlslError> {
    use glsl::syntax::UnaryOp::*;

    let result = match op {
        Minus => {
            // For matrices and vectors, the base type is used
            let base_ty = if operand_ty.is_matrix() {
                GlslType::Float // Matrix elements are always float
            } else if operand_ty.is_vector() {
                operand_ty.vector_base_type().unwrap()
            } else {
                operand_ty.clone()
            };
            match base_ty {
                GlslType::Int => ctx.builder.ins().ineg(val),
                GlslType::Float => ctx.builder.ins().fneg(val),
                _ => {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        format!("unary minus not supported for {operand_ty:?}"),
                    ));
                }
            }
        }
        Not => {
            if operand_ty != &GlslType::Bool {
                return Err(GlslError::new(
                    ErrorCode::E0107,
                    format!("logical NOT requires bool, got {operand_ty:?}"),
                ));
            }
            let zero = ctx.builder.ins().iconst(types::I8, 0);
            ctx.builder.ins().icmp(IntCC::Equal, val, zero)
        }
        Inc => {
            // Handle pre-increment directly here since dispatch isn't working
            return Err(GlslError::new(
                ErrorCode::E0400,
                "pre-increment should be handled by dispatch, but dispatch failed",
            ));
        }
        Dec => {
            // Handle pre-decrement directly here since dispatch isn't working
            return Err(GlslError::new(
                ErrorCode::E0400,
                "pre-decrement should be handled by dispatch, but dispatch failed",
            ));
        }
        _ => {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!("unary operator not supported yet: {op:?}"),
            ));
        }
    };

    Ok(result)
}
