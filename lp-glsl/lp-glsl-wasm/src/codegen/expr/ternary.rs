//! Ternary expression code generation.

use wasm_encoder::{BlockType, InstructionSink, ValType};

use crate::codegen::context::WasmCodegenContext;
use crate::codegen::expr;
use crate::codegen::rvalue::WasmRValue;
use crate::options::WasmOptions;
use lp_glsl_frontend::error::GlslDiagnostics;

/// Emit ternary: cond ? then_expr : else_expr.
pub fn emit_ternary(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    cond: &glsl::syntax::Expr,
    then_expr: &glsl::syntax::Expr,
    else_expr: &glsl::syntax::Expr,
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    let then_rv = expr::infer_expr_type(ctx, then_expr)?;
    let else_rv = expr::infer_expr_type(ctx, else_expr)?;

    if then_rv != else_rv {
        return Err(lp_glsl_frontend::error::GlslError::new(
            lp_glsl_frontend::error::ErrorCode::E0102,
            "ternary branches must have matching types",
        )
        .into());
    }

    expr::emit_rvalue(ctx, sink, cond, options)?;
    sink.i32_const(0);
    sink.i32_ne();

    sink.if_(BlockType::Result(ValType::I32));
    expr::emit_rvalue(ctx, sink, then_expr, options)?;
    sink.else_();
    expr::emit_rvalue(ctx, sink, else_expr, options)?;
    sink.end();

    Ok(WasmRValue::scalar(then_rv))
}
