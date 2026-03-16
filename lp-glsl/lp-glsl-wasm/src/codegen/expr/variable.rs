//! Variable expression code generation (local.get).

use wasm_encoder::InstructionSink;

use crate::codegen::context::WasmCodegenContext;
use lp_glsl_frontend::error::{GlslDiagnostics, GlslError};

/// Emit variable load (local.get).
pub fn emit_variable(
    ctx: &WasmCodegenContext,
    sink: &mut InstructionSink,
    expr: &glsl::syntax::Expr,
) -> Result<(), GlslDiagnostics> {
    let name = match expr {
        glsl::syntax::Expr::Variable(ident, _) => &ident.name,
        _ => {
            return Err(GlslError::new(
                lp_glsl_frontend::error::ErrorCode::E0400,
                alloc::format!("expected variable, got {:?}", expr),
            )
            .into());
        }
    };

    let info = ctx.lookup_local(name).ok_or_else(|| {
        GlslDiagnostics::from(GlslError::new(
            lp_glsl_frontend::error::ErrorCode::E0100,
            alloc::format!("undefined variable `{name}`"),
        ))
    })?;

    sink.local_get(info.index);
    Ok(())
}
