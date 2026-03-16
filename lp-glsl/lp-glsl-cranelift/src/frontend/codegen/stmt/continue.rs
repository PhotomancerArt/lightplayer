use crate::error::{ErrorCode, GlslError};
use crate::frontend::codegen::context::CodegenContext;

/// Emit continue statement
pub fn emit_continue_stmt<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
) -> Result<(), GlslError> {
    let loop_ctx = ctx
        .loop_stack
        .last()
        .ok_or_else(|| GlslError::new(ErrorCode::E0400, "continue statement outside of loop"))?;

    ctx.emit_branch(loop_ctx.continue_target)?;

    // Create unreachable block for subsequent code
    let unreachable = ctx.builder.create_block();
    ctx.emit_block(unreachable);

    Ok(())
}
