use crate::error::{ErrorCode, GlslError};
use crate::frontend::codegen::context::CodegenContext;

/// Emit break statement
pub fn emit_break_stmt<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
) -> Result<(), GlslError> {
    let loop_ctx = ctx
        .loop_stack
        .last()
        .ok_or_else(|| GlslError::new(ErrorCode::E0400, "break statement outside of loop"))?;

    ctx.emit_branch(loop_ctx.exit_block)?;

    // Create unreachable block for subsequent code
    let unreachable = ctx.builder.create_block();
    ctx.emit_block(unreachable);

    Ok(())
}
