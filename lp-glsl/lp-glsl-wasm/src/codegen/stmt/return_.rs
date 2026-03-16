//! Return statement code generation.

use crate::codegen::context::WasmCodegenContext;
use crate::codegen::expr;
use crate::options::WasmOptions;
use lp_glsl_frontend::error::GlslDiagnostics;
use lp_glsl_frontend::semantic::types::Type;

/// Emit return statement (emit expr, then return).
pub fn emit_return(
    ctx: &mut WasmCodegenContext,
    f: &mut wasm_encoder::Function,
    expr: &glsl::syntax::Expr,
    options: &WasmOptions,
    return_type: &Type,
) -> Result<(), GlslDiagnostics> {
    let mut instr = f.instructions();
    emit_return_to_sink(ctx, &mut instr, expr, options, return_type)
}

/// Emit return to instruction sink.
pub fn emit_return_to_sink(
    ctx: &mut WasmCodegenContext,
    instr: &mut wasm_encoder::InstructionSink,
    expr: &glsl::syntax::Expr,
    options: &WasmOptions,
    return_type: &Type,
) -> Result<(), GlslDiagnostics> {
    if matches!(return_type, Type::Void) {
        instr.return_();
        return Ok(());
    }
    expr::emit_rvalue(ctx, instr, expr, options)?;
    instr.return_();
    Ok(())
}
