//! Expression (rvalue) code generation.

mod assignment;
mod binary;
mod builtin_call;
pub(crate) mod builtin_inline;
mod component;
mod constructor;
mod literal;
mod ternary;
mod type_infer;
mod variable;

pub use binary::emit_binary_op;
pub use literal::emit_literal;
pub use type_infer::infer_expr_type;
pub use variable::emit_variable;

use crate::codegen::context::WasmCodegenContext;
use crate::codegen::rvalue::WasmRValue;
use crate::options::WasmOptions;
use lp_glsl_builtin_ids::glsl_q32_math_builtin_id;
use lp_glsl_frontend::FloatMode;
use lp_glsl_frontend::error::GlslDiagnostics;
use lp_glsl_frontend::semantic::builtins;
use lp_glsl_frontend::semantic::type_check::{is_scalar_type_name, is_vector_type_name};

/// Emit rvalue - expression that produces a value on the stack.
pub fn emit_rvalue(
    ctx: &mut WasmCodegenContext,
    sink: &mut wasm_encoder::InstructionSink,
    expr: &glsl::syntax::Expr,
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    use glsl::syntax::Expr;

    match expr {
        Expr::IntConst(..) | Expr::UIntConst(..) | Expr::FloatConst(..) | Expr::BoolConst(..) => {
            let ty = crate::codegen::expr::literal::emit_literal(sink, expr, ctx.numeric)?;
            Ok(WasmRValue::scalar(ty))
        }
        Expr::Variable(..) => {
            let ty = crate::codegen::expr::variable::emit_variable(ctx, sink, expr)?;
            Ok(WasmRValue::from_type(ty))
        }
        Expr::Binary(op, lhs, rhs, span) => {
            binary::emit_binary(ctx, sink, op, lhs.as_ref(), rhs.as_ref(), options, span)
        }
        Expr::Ternary(cond, then_expr, else_expr, _) => {
            ternary::emit_ternary(ctx, sink, cond, then_expr, else_expr, options)
        }
        Expr::FunCall(func_ident, args, _) => {
            let name = match func_ident {
                glsl::syntax::FunIdentifier::Identifier(ident) => ident.name.as_str(),
                _ => "",
            };
            if is_scalar_type_name(name) {
                constructor::emit_scalar_constructor(ctx, sink, name, args, options)
            } else if is_vector_type_name(name) {
                constructor::emit_vector_constructor(ctx, sink, name, args, options)
            } else if let Some(&idx) = ctx.func_index_map.get(name) {
                for arg in args.iter() {
                    emit_rvalue(ctx, sink, arg, options)?;
                }
                sink.call(idx);
                let return_ty = ctx
                    .func_return_type
                    .get(name)
                    .cloned()
                    .unwrap_or(lp_glsl_frontend::semantic::types::Type::Void);
                Ok(
                    if matches!(return_ty, lp_glsl_frontend::semantic::types::Type::Void) {
                        WasmRValue::void()
                    } else {
                        WasmRValue::from_type(return_ty)
                    },
                )
            } else if let Some(out) =
                builtin_inline::try_emit_inline_builtin(ctx, sink, expr, name, args, options)
            {
                out
            } else if options.float_mode == FloatMode::Q32
                && builtins::is_builtin_function(name)
                && glsl_q32_math_builtin_id(name, args.len()).is_some()
            {
                builtin_call::emit_q32_math_libcall(ctx, sink, expr, name, args, options)
            } else {
                Err(lp_glsl_frontend::error::GlslError::new(
                    lp_glsl_frontend::error::ErrorCode::E0400,
                    alloc::format!("fun call `{name}` not supported"),
                )
                .into())
            }
        }
        Expr::Unary(op, operand, _) => {
            use glsl::syntax::UnaryOp;
            match op {
                UnaryOp::Minus => {
                    sink.i32_const(0);
                    let operand_rv = emit_rvalue(ctx, sink, operand.as_ref(), options)?;
                    sink.i32_sub();
                    Ok(WasmRValue::scalar(operand_rv.ty))
                }
                UnaryOp::Not => {
                    emit_rvalue(ctx, sink, operand.as_ref(), options)?;
                    sink.i32_eqz();
                    Ok(WasmRValue::scalar(
                        lp_glsl_frontend::semantic::types::Type::Bool,
                    ))
                }
                _ => Err(lp_glsl_frontend::error::GlslError::new(
                    lp_glsl_frontend::error::ErrorCode::E0400,
                    alloc::format!("unary op {:?} not supported in phase ii", op),
                )
                .into()),
            }
        }
        Expr::Dot(base_expr, field, _) => {
            component::emit_dot(ctx, sink, base_expr.as_ref(), field, options)
        }
        Expr::Assignment(lhs, op, rhs, _) => {
            assignment::emit_assignment(ctx, sink, lhs.as_ref(), op, rhs.as_ref(), options)
        }
        _ => Err(lp_glsl_frontend::error::GlslError::new(
            lp_glsl_frontend::error::ErrorCode::E0400,
            alloc::format!("expr {:?} not supported in phase ii", expr),
        )
        .into()),
    }
}
