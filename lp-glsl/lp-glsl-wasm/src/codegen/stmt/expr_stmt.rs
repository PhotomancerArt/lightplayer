//! Expression statement code generation (drop result).

use crate::codegen::context::WasmCodegenContext;
use crate::codegen::expr;
use crate::options::WasmOptions;
use lp_glsl_frontend::error::GlslDiagnostics;

/// Emit expression statement (evaluate and drop result).
pub fn emit_expr_stmt(
    ctx: &mut WasmCodegenContext,
    f: &mut wasm_encoder::Function,
    expr: &glsl::syntax::Expr,
    options: &WasmOptions,
) -> Result<(), GlslDiagnostics> {
    let mut instr = f.instructions();
    emit_expr_stmt_to_sink(ctx, &mut instr, expr, options)
}

/// Emit expression statement to sink.
pub fn emit_expr_stmt_to_sink(
    ctx: &mut WasmCodegenContext,
    instr: &mut wasm_encoder::InstructionSink,
    expr: &glsl::syntax::Expr,
    options: &WasmOptions,
) -> Result<(), GlslDiagnostics> {
    let rv = expr::emit_rvalue(ctx, instr, expr, options)?;
    if rv.stack_count > 0 {
        instr.drop();
    }
    Ok(())
}
