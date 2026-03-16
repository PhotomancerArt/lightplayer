use glsl::syntax::Expr;

use crate::error::GlslError;
use crate::frontend::codegen::context::CodegenContext;

/// Emit expression statement (expression followed by semicolon)
pub fn emit_expr_stmt<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    expr: &Expr,
) -> Result<(), GlslError> {
    // Use emit_rvalue instead of translate_expr to properly handle void function calls
    // which produce zero values but are valid as statements
    let _rvalue = ctx.emit_rvalue(expr)?;
    Ok(())
}
