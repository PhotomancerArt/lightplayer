//! Variable expression code generation (local.get).

use wasm_encoder::InstructionSink;

use crate::codegen::context::WasmCodegenContext;
use crate::codegen::expr::literal::emit_const_value;
use lp_glsl_frontend::error::{GlslDiagnostics, GlslError};
use lp_glsl_frontend::semantic::types::Type;

/// Emit variable load (local.get) or inline module `const` value. Returns the variable's type.
pub fn emit_variable(
    ctx: &WasmCodegenContext,
    sink: &mut InstructionSink,
    expr: &glsl::syntax::Expr,
) -> Result<Type, GlslDiagnostics> {
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

    if let Some(val) = ctx.global_constants.get(name) {
        return emit_const_value(sink, val, ctx.numeric);
    }

    let info = ctx.lookup_local(name).ok_or_else(|| {
        GlslDiagnostics::from(GlslError::new(
            lp_glsl_frontend::error::ErrorCode::E0100,
            alloc::format!("undefined variable `{name}`"),
        ))
    })?;

    for i in 0..info.component_count {
        sink.local_get(info.base_index + i);
    }
    Ok(info.ty.clone())
}
