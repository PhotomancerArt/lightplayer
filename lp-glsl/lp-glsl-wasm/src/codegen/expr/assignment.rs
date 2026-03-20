//! Assignment expression code generation.

use glsl::syntax::{AssignmentOp, Expr};

use crate::codegen::context::WasmCodegenContext;
use crate::codegen::expr::{self, infer_expr_type};
use crate::codegen::rvalue::WasmRValue;
use crate::options::WasmOptions;
use lp_glsl_frontend::error::{GlslDiagnostics, extract_span_from_expr};

/// Store values popped from the operand stack into `lhs` (variable). For vectors, the top of
/// stack is the last component; matches [`emit_simple_assignment`] ordering.
pub(crate) fn emit_store_stack_into_lvalue(
    ctx: &WasmCodegenContext,
    sink: &mut wasm_encoder::InstructionSink,
    lhs: &Expr,
) -> Result<(), GlslDiagnostics> {
    let (name, component_count) = match lhs {
        Expr::Variable(ident, _) => {
            let info = ctx.lookup_local(&ident.name).ok_or_else(|| {
                GlslDiagnostics::from(lp_glsl_frontend::error::GlslError::new(
                    lp_glsl_frontend::error::ErrorCode::E0100,
                    alloc::format!("undefined variable `{}`", ident.name),
                ))
            })?;
            (ident.name.clone(), info.component_count)
        }
        _ => {
            return Err(lp_glsl_frontend::error::GlslError::new(
                lp_glsl_frontend::error::ErrorCode::E0115,
                "inout/out argument must be an assignable variable",
            )
            .into());
        }
    };
    let info = ctx.lookup_local(&name).unwrap();
    let base_index = info.base_index;
    if component_count == 1 {
        sink.local_set(base_index);
    } else {
        for i in (0..component_count).rev() {
            sink.local_set(base_index + i);
        }
    }
    Ok(())
}

/// Emit assignment expression. Returns WasmRValue with lhs type.
pub fn emit_assignment(
    ctx: &mut WasmCodegenContext,
    sink: &mut wasm_encoder::InstructionSink,
    lhs: &Expr,
    op: &AssignmentOp,
    rhs: &Expr,
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    if matches!(op, AssignmentOp::Equal) {
        emit_simple_assignment(ctx, sink, lhs, rhs, options)
    } else {
        emit_compound_assignment(ctx, sink, lhs, op, rhs, options)
    }
}

fn emit_simple_assignment(
    ctx: &mut WasmCodegenContext,
    sink: &mut wasm_encoder::InstructionSink,
    lhs: &Expr,
    rhs: &Expr,
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    let (name, ty) = match lhs {
        Expr::Variable(ident, _) => {
            let info = ctx.lookup_local(&ident.name).ok_or_else(|| {
                GlslDiagnostics::from(lp_glsl_frontend::error::GlslError::new(
                    lp_glsl_frontend::error::ErrorCode::E0100,
                    alloc::format!("undefined variable `{}`", ident.name),
                ))
            })?;
            (ident.name.clone(), info.ty.clone())
        }
        _ => {
            return Err(lp_glsl_frontend::error::GlslError::new(
                lp_glsl_frontend::error::ErrorCode::E0115,
                "assignment to non-variable not supported",
            )
            .into());
        }
    };

    let (base_index, component_count) = {
        let info = ctx.lookup_local(&name).unwrap();
        (info.base_index, info.component_count)
    };
    expr::emit_rvalue(ctx, sink, rhs, options)?;
    if component_count == 1 {
        sink.local_tee(base_index);
        Ok(WasmRValue::from_type(ty))
    } else {
        for i in (0..component_count).rev() {
            sink.local_set(base_index + i);
        }
        Ok(WasmRValue::void())
    }
}

fn emit_compound_assignment(
    ctx: &mut WasmCodegenContext,
    sink: &mut wasm_encoder::InstructionSink,
    lhs: &Expr,
    op: &AssignmentOp,
    rhs: &Expr,
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    use glsl::syntax::BinaryOp;

    let (name, ty) = match lhs {
        Expr::Variable(ident, _) => {
            let info = ctx.lookup_local(&ident.name).ok_or_else(|| {
                GlslDiagnostics::from(lp_glsl_frontend::error::GlslError::new(
                    lp_glsl_frontend::error::ErrorCode::E0100,
                    alloc::format!("undefined variable `{}`", ident.name),
                ))
            })?;
            (ident.name.clone(), info.ty.clone())
        }
        _ => {
            return Err(lp_glsl_frontend::error::GlslError::new(
                lp_glsl_frontend::error::ErrorCode::E0115,
                "compound assignment to non-variable not supported",
            )
            .into());
        }
    };

    let (base_index, component_count) = {
        let info = ctx.lookup_local(&name).unwrap();
        (info.base_index, info.component_count)
    };
    let binary_op = match op {
        AssignmentOp::Add => BinaryOp::Add,
        AssignmentOp::Sub => BinaryOp::Sub,
        AssignmentOp::Mult => BinaryOp::Mult,
        AssignmentOp::Div => BinaryOp::Div,
        _ => {
            return Err(lp_glsl_frontend::error::GlslError::new(
                lp_glsl_frontend::error::ErrorCode::E0400,
                alloc::format!("compound assignment {:?} not supported", op),
            )
            .into());
        }
    };

    let rhs_ty = infer_expr_type(ctx, rhs)?;
    if component_count == 1 {
        for i in 0..component_count {
            sink.local_get(base_index + i);
        }
        expr::emit_rvalue(ctx, sink, rhs, options)?;
        let numeric = crate::codegen::numeric::WasmNumericMode::from(options.float_mode);
        crate::codegen::expr::binary::emit_binary_op(ctx, sink, &binary_op, &ty, &rhs_ty, numeric)?;
        sink.local_tee(base_index);
        Ok(WasmRValue::from_type(ty))
    } else {
        let span = extract_span_from_expr(lhs);
        let _ = crate::codegen::expr::binary::emit_binary(
            ctx, sink, &binary_op, lhs, rhs, options, &span,
        )?;
        for i in (0..component_count).rev() {
            sink.local_set(base_index + i);
        }
        Ok(WasmRValue::void())
    }
}
