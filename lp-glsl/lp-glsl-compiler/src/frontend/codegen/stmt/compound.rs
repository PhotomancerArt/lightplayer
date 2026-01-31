use glsl::syntax::CompoundStatement;

use crate::error::GlslError;
use crate::frontend::codegen::context::CodegenContext;

/// Emit compound statement (block of statements in { ... })
pub fn emit_compound_stmt<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    compound: &CompoundStatement,
) -> Result<(), GlslError> {
    ctx.enter_scope(); // Enter scope for compound block
    // Translate all statements in the compound block
    for stmt in &compound.statement_list {
        ctx.emit_statement(stmt)?;
    }
    ctx.exit_scope(); // Exit scope for compound block
    Ok(())
}
