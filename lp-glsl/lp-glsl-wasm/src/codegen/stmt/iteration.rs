//! Iteration statement dispatch and condition helper.

use crate::codegen::context::WasmCodegenContext;
use crate::codegen::expr;
use crate::options::WasmOptions;
use lp_glsl_frontend::error::GlslDiagnostics;
use wasm_encoder::InstructionSink;

/// Emit condition to bool (i32 on stack). Supports Condition::Expr only for now.
pub fn emit_condition_to_sink(
    ctx: &mut WasmCodegenContext,
    instr: &mut InstructionSink,
    condition: &glsl::syntax::Condition,
    options: &WasmOptions,
) -> Result<(), GlslDiagnostics> {
    match condition {
        glsl::syntax::Condition::Expr(expr) => {
            expr::emit_rvalue(ctx, instr, expr, options)?;
            // Value is already i32 (bool). No coercion needed.
            Ok(())
        }
        glsl::syntax::Condition::Assignment(..) => Err(lp_glsl_frontend::error::GlslError::new(
            lp_glsl_frontend::error::ErrorCode::E0400,
            "variable declaration in loop condition not yet supported",
        )
        .into()),
    }
}
