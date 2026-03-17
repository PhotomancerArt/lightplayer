//! Expression (rvalue) code generation.

mod binary;
mod literal;
mod variable;

pub use binary::emit_binary_op;
pub use literal::emit_literal;
pub use variable::emit_variable;

use crate::codegen::context::WasmCodegenContext;
use crate::options::WasmOptions;
use lp_glsl_frontend::error::GlslDiagnostics;

/// Emit rvalue - expression that produces a value on the stack.
pub fn emit_rvalue(
    ctx: &mut WasmCodegenContext,
    sink: &mut wasm_encoder::InstructionSink,
    expr: &glsl::syntax::Expr,
    options: &WasmOptions,
) -> Result<(), GlslDiagnostics> {
    use glsl::syntax::Expr;

    match expr {
        Expr::IntConst(..) | Expr::UIntConst(..) | Expr::FloatConst(..) | Expr::BoolConst(..) => {
            crate::codegen::expr::literal::emit_literal(sink, expr, ctx.numeric)?;
        }
        Expr::Variable(..) => {
            crate::codegen::expr::variable::emit_variable(ctx, sink, expr)?;
        }
        Expr::Binary(op, lhs, rhs, _) => {
            emit_rvalue(ctx, sink, lhs.as_ref(), options)?;
            emit_rvalue(ctx, sink, rhs.as_ref(), options)?;
            let numeric = crate::codegen::numeric::WasmNumericMode::from(options.float_mode);
            crate::codegen::expr::binary::emit_binary_op(sink, op, numeric)?;
        }
        Expr::Unary(op, operand, _) => {
            use glsl::syntax::UnaryOp;
            match op {
                UnaryOp::Minus => {
                    emit_rvalue(ctx, sink, operand.as_ref(), options)?;
                    sink.i32_const(0);
                    sink.i32_sub();
                }
                UnaryOp::Not => {
                    emit_rvalue(ctx, sink, operand.as_ref(), options)?;
                    sink.i32_eqz();
                }
                _ => {
                    return Err(lp_glsl_frontend::error::GlslError::new(
                        lp_glsl_frontend::error::ErrorCode::E0400,
                        alloc::format!("unary op {:?} not supported in phase ii", op),
                    )
                    .into());
                }
            }
        }
        _ => {
            return Err(lp_glsl_frontend::error::GlslError::new(
                lp_glsl_frontend::error::ErrorCode::E0400,
                alloc::format!("expr {:?} not supported in phase ii", expr),
            )
            .into());
        }
    }
    Ok(())
}
